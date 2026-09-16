use std::path::PathBuf;
use std::time::Duration;

use super::local::parse_control_paragraphs;
use crate::backend::common::{FetchOutcome, FetchSummary, RefreshStats};
use crate::error::Result;
use crate::model::*;
use crate::network::FetchRequest;

#[derive(Debug, Clone)]
pub struct AptSource {
    pub url: String,
    pub distribution: String,
    pub components: Vec<String>,
    pub arch: String,
    pub repo_label: String,
}

#[derive(Debug, Clone)]
struct AuthEntry {
    machine: String,
    login: String,
    password: String,
}

pub fn parse_apt_sources() -> Vec<AptSource> {
    let mut sources = Vec::new();
    let archs = detect_archs();

    let main_list = PathBuf::from("/etc/apt/sources.list");
    if let Ok(content) = std::fs::read_to_string(&main_list) {
        for arch in &archs {
            sources.extend(parse_sources_content(&content, arch));
        }
    }

    let sources_d = PathBuf::from("/etc/apt/sources.list.d");
    if let Ok(entries) = std::fs::read_dir(&sources_d) {
        for entry in entries.flatten() {
            let path = entry.path();
            match path.extension().and_then(|e| e.to_str()) {
                // 传统 one-line 格式
                Some("list") => {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for arch in &archs {
                            sources.extend(parse_sources_content(&content, arch));
                        }
                    }
                }
                // deb822 格式（Debian 13+/Ubuntu 24.04+ 默认，P0-4）
                Some("sources") => {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for arch in &archs {
                            sources.extend(parse_deb822_sources(&content, arch));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    sources
}

fn detect_archs() -> Vec<String> {
    let dpkg_arch_path = PathBuf::from("/var/lib/dpkg/arch");
    if let Ok(content) = std::fs::read_to_string(&dpkg_arch_path) {
        let archs: Vec<String> = content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if !archs.is_empty() {
            return archs;
        }
    }
    vec![std::env::consts::ARCH.to_string()]
}

fn parse_sources_content(content: &str, arch: &str) -> Vec<AptSource> {
    let mut sources = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if !line.starts_with("deb ") && !line.starts_with("deb\t") {
            continue;
        }

        let rest = line[4..].trim();

        let rest = if rest.starts_with('[') {
            let end = rest.find(']').map(|e| e + 1).unwrap_or(0);
            rest[end..].trim_start()
        } else {
            rest
        };

        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let url = parts[0].trim_end_matches('/').to_string();
        let distribution = parts[1].to_string();
        let components: Vec<String> = parts[2..].iter().map(|s| s.to_string()).collect();

        let host = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(&url)
            .split('/')
            .next()
            .unwrap_or(&url)
            .to_string();
        let repo_label = format!("{} {}", host, distribution);

        sources.push(AptSource {
            url,
            distribution,
            components,
            arch: arch.to_string(),
            repo_label,
        });
    }

    sources
}

/// 解析 deb822 格式（.sources，Debian 13/Ubuntu 24.04+ 默认，P0-4）
/// 支持多行字段续行、Types/URIs/Suites/Components/Architectures；deb-src 跳过
pub fn parse_deb822_sources(content: &str, arch: &str) -> Vec<AptSource> {
    let mut sources = Vec::new();
    // stanza: Vec<(key, Vec<续行合并后的值>)>，字段可重复出现
    let mut stanzas: Vec<Vec<(String, String)>> = Vec::new();
    let mut current: Vec<(String, String)> = Vec::new();
    let mut last_key: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                stanzas.push(std::mem::take(&mut current));
            }
            last_key = None;
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        // 续行
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(k) = &last_key {
                if let Some(entry) = current.iter_mut().find(|(key, _)| key == k) {
                    entry.1.push(' ');
                    entry.1.push_str(trimmed);
                }
            }
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim().to_string();
            current.push((k.clone(), v.trim().to_string()));
            last_key = Some(k);
        }
    }
    if !current.is_empty() {
        stanzas.push(current);
    }

    for stanza in stanzas {
        let types = get_field(stanza.as_slice(), "Types").unwrap_or_default();
        if !types.split_whitespace().any(|t| t == "deb") {
            continue; // deb-src-only
        }
        if let Some(arches) = get_field(stanza.as_slice(), "Architectures") {
            if !arches
                .split_whitespace()
                .any(|a| a == arch || a == "any" || a == "all")
            {
                continue;
            }
        }
        let uris = get_field(stanza.as_slice(), "URIs").unwrap_or_default();
        let suites = get_field(stanza.as_slice(), "Suites").unwrap_or_default();
        let components: Vec<String> = get_field(stanza.as_slice(), "Components")
            .unwrap_or_default()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        for uri in uris.split_whitespace() {
            let url = uri.trim_end_matches('/').to_string();
            let host = url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .unwrap_or(&url)
                .split('/')
                .next()
                .unwrap_or(&url)
                .to_string();
            for suite in suites.split_whitespace() {
                let repo_label = format!("{} {}", host, suite);
                sources.push(AptSource {
                    url: url.clone(),
                    distribution: suite.to_string(),
                    components: components.clone(),
                    arch: arch.to_string(),
                    repo_label,
                });
            }
        }
    }

    sources
}

fn get_field(stanza: &[(String, String)], key: &str) -> Option<String> {
    stanza
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

fn parse_auth_conf() -> Vec<AuthEntry> {
    let mut entries = Vec::new();

    let auth_dir = PathBuf::from("/etc/apt/auth.conf.d");
    if let Ok(dir_entries) = std::fs::read_dir(&auth_dir) {
        for entry in dir_entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                entries.extend(parse_auth_content(&content));
            }
        }
    }

    let auth_file = PathBuf::from("/etc/apt/auth.conf");
    if let Ok(content) = std::fs::read_to_string(&auth_file) {
        entries.extend(parse_auth_content(&content));
    }

    entries
}

fn parse_auth_content(content: &str) -> Vec<AuthEntry> {
    let mut entries = Vec::new();
    let mut current_machine = String::new();
    let mut current_login = String::new();
    let mut current_password = String::new();

    let flush = |entries: &mut Vec<AuthEntry>, machine: &str, login: &str, password: &str| {
        if !machine.is_empty() {
            entries.push(AuthEntry {
                machine: machine.to_string(),
                login: login.to_string(),
                password: password.to_string(),
            });
        }
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            flush(
                &mut entries,
                &current_machine,
                &current_login,
                &current_password,
            );
            current_machine.clear();
            current_login.clear();
            current_password.clear();
            continue;
        }

        if let Some(rest) = line.strip_prefix("machine ") {
            if !current_machine.is_empty() {
                flush(
                    &mut entries,
                    &current_machine,
                    &current_login,
                    &current_password,
                );
                current_login.clear();
                current_password.clear();
            }
            current_machine = rest.split_whitespace().next().unwrap_or("").to_string();
        } else if let Some(rest) = line.strip_prefix("login ") {
            current_login = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("password ") {
            current_password = rest.trim().to_string();
        }
    }

    flush(
        &mut entries,
        &current_machine,
        &current_login,
        &current_password,
    );

    entries
}

fn find_auth_for_url(url: &str, auth_entries: &[AuthEntry]) -> Option<(String, String)> {
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = host.split('/').next().unwrap_or(host);

    let mut best_match: Option<&AuthEntry> = None;
    for entry in auth_entries {
        if (host == entry.machine || host.ends_with(&format!(".{}", entry.machine)))
            && (best_match.is_none() || entry.machine.len() > best_match.unwrap().machine.len())
        {
            best_match = Some(entry);
        }
    }

    best_match.map(|e| (e.login.clone(), e.password.clone()))
}

/// 解析 apt 的 `mirror+file:` URI 方案：从指定的镜像清单文件取首个
/// 镜像 URL（GitHub Actions runner 等云镜像环境的 ubuntu.sources 用此
/// 方案指向本地镜像列表，直接当 http URL 拼接会导致所有源秒失败）。
/// 非 mirror+file: 的 URI 原样返回；清单不可读/为空时返回原 URI，
/// 由后续网络错误路径兜底。
fn resolve_mirror_uri(uri: &str) -> String {
    let Some(path) = uri.strip_prefix("mirror+file:") else {
        return uri.to_string();
    };
    match std::fs::read_to_string(path) {
        Ok(content) => content
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            // 行格式为 "URI[ \t]priority:N"（GitHub runner 实测），取首字段
            .and_then(|l| l.split_whitespace().next())
            .map(|l| l.trim_end_matches('/').to_string())
            .unwrap_or_else(|| uri.to_string()),
        Err(_) => uri.to_string(),
    }
}

fn apt_lists_filename(source: &AptSource, component: &str) -> String {
    // apt 落盘命名不含 mirror+file: 方案前缀（scheme 剥离后按路径转写，
    // 如 mirror+file:/etc/apt/apt-mirrors.txt → _etc_apt_apt-mirrors.txt），
    // 保持一致才能命中 /var/lib/apt/lists 的本地回退
    let url = source
        .url
        .strip_prefix("mirror+file:")
        .unwrap_or(&source.url);
    let prefix = url
        .replace("https://", "")
        .replace("http://", "")
        .replace(['/', ':'], "_");
    let dist = source.distribution.replace('/', "_");
    format!(
        "{}_dists_{}_{}_binary-{}_Packages",
        prefix, dist, component, source.arch
    )
}

pub struct DebOnline {
    auth_entries: Vec<AuthEntry>,
    apt_lists_dir: PathBuf,
    cache: crate::network::NetworkCacheManager,
}

impl Default for DebOnline {
    fn default() -> Self {
        Self::new()
    }
}

impl DebOnline {
    pub fn new() -> Self {
        Self {
            auth_entries: parse_auth_conf(),
            apt_lists_dir: PathBuf::from("/var/lib/apt/lists"),
            cache: crate::network::NetworkCacheManager::new(),
        }
    }

    /// 获取指定源/组件的 Packages 数据（P0-3 真联网改造）：
    /// 1. pkq 自有缓存（~/.cache/pkq/repos/，ETag/TTL 管理）
    /// 2. 过期/强制时原生 HTTP 拉取 Packages.gz 并写回缓存
    /// 3. 联网失败回退 /var/lib/apt/lists（静默降级）
    /// 4. --offline：自有缓存 → apt lists → 报错
    ///
    /// 拉取单个源的 Packages 并解析。
    ///
    /// 返回值携带来源（Online / LocalFallback），由调用方统一渲染进度。
    pub fn fetch_repo_packages(
        &self,
        source: &AptSource,
        component: &str,
        ttl: Duration,
        force: bool,
        offline: bool,
    ) -> Result<(Vec<PkgMetadata>, FetchOutcome)> {
        let repo_label = format!("{}/{}", source.repo_label, component);
        let lists_file = self
            .apt_lists_dir
            .join(apt_lists_filename(source, component));
        let packages_url = format!(
            "{}/dists/{}/{}/binary-{}/Packages.gz",
            resolve_mirror_uri(&source.url),
            source.distribution,
            component,
            source.arch
        );
        tracing::debug!("fetching packages index: {}", packages_url);

        let auth = find_auth_for_url(&source.url, &self.auth_entries);
        let (user, pass) = match &auth {
            Some((u, p)) => (Some(u.as_str()), Some(p.as_str())),
            None => (None, None),
        };

        // 缓存 repo_id：复用 lists 文件名（天然唯一且路径安全）
        let cache_id = apt_lists_filename(source, component);
        match self.cache.fetch_index(FetchRequest {
            repo_id: &cache_id,
            url: &packages_url,
            ttl,
            force,
            offline,
            username: user,
            password: pass,
        }) {
            Ok(path) => {
                let data = std::fs::read(&path)?;
                let content = decompress_gz_bytes(&data)?;
                let mut pkgs = parse_control_paragraphs(&content);
                for p in &mut pkgs {
                    p.source_repo = Some(repo_label.clone());
                }
                Ok((pkgs, FetchOutcome::Online))
            }
            Err(e) => {
                if lists_file.exists() {
                    // 在线失败但本地 apt lists 可用：降级成功。数据可能过期，
                    // 原因压缩后随返回值交由调用方渲染
                    let reason = crate::backend::common::brief_network_reason(&e.to_string());
                    let data = std::fs::read(&lists_file)?;
                    let content = String::from_utf8_lossy(&data).to_string();
                    let mut pkgs = parse_control_paragraphs(&content);
                    for p in &mut pkgs {
                        p.source_repo = Some(repo_label.clone());
                    }
                    return Ok((pkgs, FetchOutcome::LocalFallback(reason)));
                }
                Err(e)
            }
        }
    }

    pub fn fetch_all_repos(
        &self,
        sources: &[AptSource],
        ttl: Duration,
        force: bool,
        offline: bool,
        quiet: bool,
    ) -> FetchSummary {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tasks: Vec<(&AptSource, &String)> = sources
            .iter()
            .flat_map(|s| s.components.iter().map(move |c| (s, c)))
            .collect();
        let total = tasks.len();
        let done = AtomicUsize::new(0);
        let (n_online, n_fb, n_fail) = (
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        );
        if !quiet && total > 0 {
            eprintln!("正在刷新仓库元数据（共 {} 个源任务）...", total);
        }

        let results: Vec<Vec<PkgMetadata>> = tasks
            .par_iter()
            .filter_map(|(source, component)| {
                let outcome = self.fetch_repo_packages(source, component, ttl, force, offline);
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                // label：发行版/组件 (架构) @ 主机——每任务唯一可辨
                let host = source
                    .repo_label
                    .split(' ')
                    .next()
                    .unwrap_or(&source.repo_label);
                let label = format!(
                    "{}/{} ({}) @ {}",
                    source.distribution, component, source.arch, host
                );
                if !quiet {
                    match &outcome {
                        Ok((_, FetchOutcome::Online)) => {
                            n_online.fetch_add(1, Ordering::SeqCst);
                            eprintln!("  ✓ [{:>2}/{}] {}", n, total, label);
                        }
                        Ok((_, FetchOutcome::LocalFallback(reason))) => {
                            n_fb.fetch_add(1, Ordering::SeqCst);
                            eprintln!(
                                "  △ [{:>2}/{}] {} —— {}，已回退本地缓存",
                                n, total, label, reason
                            );
                        }
                        Err(e) => {
                            n_fail.fetch_add(1, Ordering::SeqCst);
                            let reason =
                                crate::backend::common::brief_network_reason(&e.to_string());
                            eprintln!("  ✗ [{:>2}/{}] {} —— {}", n, total, label, reason);
                        }
                    }
                }
                outcome.ok().map(|(pkgs, _)| pkgs)
            })
            .collect();

        let stats = RefreshStats {
            total,
            online: n_online.load(Ordering::SeqCst),
            fallback: n_fb.load(Ordering::SeqCst),
            failed: n_fail.load(Ordering::SeqCst),
        };
        if !quiet && total > 0 {
            if stats.all_online() {
                eprintln!("  源刷新完成：{} 个源全部在线成功", total);
            } else if stats.fallback > 0 {
                eprintln!(
                    "  源刷新完成：在线成功 {} 个，刷新失败 {} 个（部分已回退本地缓存，数据可能过期）",
                    stats.online,
                    stats.unsuccessful()
                );
            } else {
                eprintln!(
                    "  源刷新完成：在线成功 {} 个，刷新失败 {} 个（无本地缓存可回退）",
                    stats.online,
                    stats.unsuccessful()
                );
            }
        }

        let package_count: usize = results.iter().map(|r| r.len()).sum();
        let mut all = Vec::with_capacity(package_count);
        for r in results {
            all.extend(r);
        }
        FetchSummary {
            packages: all,
            stats,
            package_count,
        }
    }

    pub fn search_contents(&self, pattern: &str, use_regex: bool, arch: &str) -> Vec<SearchResult> {
        let re = if use_regex {
            match regex::Regex::new(pattern) {
                Ok(r) => Some(r),
                Err(_) => return Vec::new(),
            }
        } else {
            None
        };

        let pattern_lower = pattern.to_lowercase();
        let noise_exts = [
            ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".bmp", ".html", ".htm", ".css",
        ];
        let file_match_fn = |search_path: &str| -> bool {
            if let Some(ref re) = re {
                return re.is_match(search_path);
            }
            // 完整路径查询：整路径精确匹配（P0-2 修复：原先与段比较永不命中）
            if pattern_lower.starts_with('/') {
                return search_path.trim_end_matches('/') == pattern_lower.trim_end_matches('/');
            }
            let path = search_path.to_lowercase();
            if noise_exts.iter().any(|ext| path.ends_with(ext)) {
                return false;
            }
            let segs: Vec<&str> = path.split('/').collect();
            for seg in &segs {
                if seg == &pattern_lower {
                    return true;
                }
                if let Some(rest) = seg.strip_prefix(&pattern_lower) {
                    if rest.is_empty()
                        || rest.starts_with('.')
                        || rest.starts_with('-')
                        || rest.starts_with('_')
                    {
                        return true;
                    }
                }
                if let Some(rest) = seg.strip_suffix(&pattern_lower) {
                    if rest.is_empty()
                        || rest.ends_with('.')
                        || rest.ends_with('-')
                        || rest.ends_with('_')
                    {
                        return true;
                    }
                }
            }
            false
        };

        let mut results = Vec::new();
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        if let Ok(entries) = std::fs::read_dir(&self.apt_lists_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if !name_str.contains("Contents-") || !name_str.ends_with(".lz4") {
                    continue;
                }
                if !name_str.contains(&format!("-{}", arch)) && !name_str.contains("Contents-all") {
                    continue;
                }

                if let Ok(data) = std::fs::read(entry.path()) {
                    let decompressed = decompress_lz4_frame(&data);
                    if let Ok(content) = decompressed {
                        for line in content.lines() {
                            let line = line.trim();
                            if line.is_empty() || line.starts_with("FILE") {
                                continue;
                            }
                            let parts: Vec<&str> = line.splitn(2, '\t').collect();
                            if parts.len() != 2 {
                                continue;
                            }
                            let file_path = parts[0];
                            let pkg_field = parts[1];

                            let search_path = if file_path.starts_with('/') {
                                file_path.to_string()
                            } else {
                                format!("/{}", file_path)
                            };

                            if file_match_fn(&search_path) {
                                for pkg_entry in pkg_field.split(',') {
                                    let pkg_name = pkg_entry
                                        .split('/')
                                        .next_back()
                                        .unwrap_or(pkg_entry)
                                        .trim();
                                    if !pkg_name.is_empty() {
                                        let key = (pkg_name.to_string(), search_path.clone());
                                        if seen.insert(key) {
                                            results.push(SearchResult {
                                                pkg_name: pkg_name.to_string(),
                                                matched_text: search_path.clone(),
                                                match_type: "file".to_string(),
                                                source: "repo".to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    pub fn collect_all_contents_raw(&self, arch: &str) -> Vec<u8> {
        use rayon::prelude::*;

        let files: Vec<PathBuf> = std::fs::read_dir(&self.apt_lists_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.to_string_lossy();
                name.contains("Contents-")
                    && name.ends_with(".lz4")
                    && (name.contains(&format!("-{}", arch)) || name.contains("Contents-all"))
            })
            .collect();

        let parts: Vec<Vec<u8>> = files
            .par_iter()
            .filter_map(|path| {
                let file_data = std::fs::read(path).ok()?;
                let content = decompress_lz4_frame(&file_data).ok()?;
                Some(content.into_bytes())
            })
            .collect();

        let total_size: usize = parts.iter().map(|p| p.len()).sum();
        let mut data = Vec::with_capacity(total_size + parts.len());
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                data.push(b'\n');
            }
            data.extend_from_slice(&part);
        }
        data
    }

    pub fn collect_all_contents(&self, arch: &str) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        if let Ok(dir_entries) = std::fs::read_dir(&self.apt_lists_dir) {
            for entry in dir_entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if !name_str.contains("Contents-") || !name_str.ends_with(".lz4") {
                    continue;
                }
                if !name_str.contains(&format!("-{}", arch)) && !name_str.contains("Contents-all") {
                    continue;
                }

                if let Ok(data) = std::fs::read(entry.path()) {
                    if let Ok(content) = decompress_lz4_frame(&data) {
                        for line in content.lines() {
                            let line = line.trim();
                            if line.is_empty() || line.starts_with("FILE") {
                                continue;
                            }
                            let parts: Vec<&str> = line.splitn(2, '\t').collect();
                            if parts.len() != 2 {
                                continue;
                            }
                            let file_path = parts[0];
                            let pkg_field = parts[1];

                            let search_path = if file_path.starts_with('/') {
                                file_path.to_string()
                            } else {
                                format!("/{}", file_path)
                            };

                            for pkg_entry in pkg_field.split(',') {
                                let pkg_name =
                                    pkg_entry.split('/').next_back().unwrap_or(pkg_entry).trim();
                                if !pkg_name.is_empty() {
                                    let key = (pkg_name.to_string(), search_path.clone());
                                    if seen.insert(key) {
                                        entries.push((search_path.clone(), pkg_name.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        entries
    }
}

fn decompress_gz_bytes(data: &[u8]) -> Result<String> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(data);
    let mut content = Vec::new();
    decoder.read_to_end(&mut content)?;
    Ok(String::from_utf8_lossy(&content).to_string())
}

fn decompress_lz4_frame(data: &[u8]) -> std::io::Result<String> {
    use std::io::Read;

    let mut decoder = lz4_flex::frame::FrameDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sources() {
        let content = r#"# comment
deb https://example.com/repo jammy main contrib non-free
# deb-src https://example.com/repo jammy main
deb [trusted=yes] https://other.com/repo bullseye main
"#;
        let sources = parse_sources_content(content, "amd64");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].url, "https://example.com/repo");
        assert_eq!(sources[0].distribution, "jammy");
        assert_eq!(sources[0].components.len(), 3);
    }

    #[test]
    fn test_parse_auth() {
        let content = "machine example.com\nlogin user\npassword pass123";
        let entries = parse_auth_content(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].machine, "example.com");
        assert_eq!(entries[0].login, "user");
        assert_eq!(entries[0].password, "pass123");
    }

    #[test]
    fn test_resolve_mirror_uri() {
        // 非 mirror+file: 原样返回
        assert_eq!(
            resolve_mirror_uri("https://deb.debian.org/debian"),
            "https://deb.debian.org/debian"
        );
        // 清单不可读：原样返回（由网络错误路径兜底）
        assert_eq!(
            resolve_mirror_uri("mirror+file:/nonexistent/mirrors.txt"),
            "mirror+file:/nonexistent/mirrors.txt"
        );
        // 取首个非注释镜像行的首字段（行可带 "URI<TAB>priority:N" 优先级后缀），去尾斜杠
        let dir = std::env::temp_dir().join(format!("pkq_mirror_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mf = dir.join("mirrors.txt");
        std::fs::write(
            &mf,
            "# comment\nhttp://azure.archive.ubuntu.com/ubuntu/\tpriority:1\nhttp://backup/ubuntu\n",
        )
        .unwrap();
        let uri = format!("mirror+file:{}", mf.display());
        assert_eq!(
            resolve_mirror_uri(&uri),
            "http://azure.archive.ubuntu.com/ubuntu"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_apt_lists_filename_mirror_scheme() {
        // 与 apt 落盘命名对齐：scheme 剥离后按路径转写
        let src = AptSource {
            url: "mirror+file:/etc/apt/apt-mirrors.txt".to_string(),
            distribution: "noble".to_string(),
            components: vec!["main".to_string()],
            arch: "amd64".to_string(),
            repo_label: "test".to_string(),
        };
        assert_eq!(
            apt_lists_filename(&src, "main"),
            "_etc_apt_apt-mirrors.txt_dists_noble_main_binary-amd64_Packages"
        );
    }

    #[test]
    fn test_parse_deb822_sources() {
        let content = "\
Types: deb
URIs: https://deb.debian.org/debian
Suites: trixie stable
Components: main contrib

Types: deb deb-src
URIs: https://example.com/repo
Suites: jammy
Components: main
Architectures: amd64 arm64

Types: deb-src
URIs: https://src-only.com
Suites: sid
Components: main
";
        let sources = parse_deb822_sources(content, "amd64");
        // stanza1: 1 uri × 2 suites = 2；stanza2: arch 匹配 → 1；stanza3: deb-src-only 跳过
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].url, "https://deb.debian.org/debian");
        assert_eq!(sources[0].distribution, "trixie");
        assert_eq!(sources[0].components, vec!["main", "contrib"]);
        assert_eq!(sources[1].distribution, "stable");
        assert_eq!(sources[2].distribution, "jammy");
    }

    #[test]
    fn test_parse_deb822_arch_mismatch() {
        let content =
            "Types: deb\nURIs: https://a.com\nSuites: x\nComponents: main\nArchitectures: arm64\n";
        assert!(parse_deb822_sources(content, "amd64").is_empty());
    }

    #[test]
    fn test_apt_lists_filename() {
        let source = AptSource {
            url: "https://apt.example.com/desktop-professional".to_string(),
            distribution: "eagle".to_string(),
            components: vec!["main".to_string()],
            arch: "amd64".to_string(),
            repo_label: "apt.example.com eagle/main".to_string(),
        };
        let filename = apt_lists_filename(&source, "main");
        assert_eq!(
            filename,
            "apt.example.com_desktop-professional_dists_eagle_main_binary-amd64_Packages"
        );
    }
}

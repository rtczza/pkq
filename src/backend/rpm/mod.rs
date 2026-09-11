pub mod local;
pub mod online;
pub mod repo_parser;

use std::time::Duration;

use crate::backend::common::FetchOutcome;
use crate::backend::common::{RefreshReport, RefreshStats};
use crate::backend::PkgBackend;
use crate::cache;
use crate::error::{PkgError, Result};
use crate::model::*;
use crate::network::{FetchRequest, RepoQuery};
use online::RpmOnline;
use repo_parser::{parse_repo_files, resolve_url, RepoConfig};

use std::sync::{Mutex, OnceLock};

/// 仓库侧文件索引：`file_path\tpkg_name\n` 扁平字节流（来自 filelists.xml）。
///
/// P1：解析结果落盘为扁平文本，进程内以只读 mmap 检索（对齐 DEB Contents），
/// 避免把全部 filelists 载入堆内存，也避免每次调用重复解析 XML。
pub struct RpmFileIndex {
    data: Option<memmap2::Mmap>,
}

impl RpmFileIndex {
    fn empty() -> Self {
        Self { data: None }
    }

    fn lines(&self) -> impl Iterator<Item = &[u8]> {
        self.data
            .as_ref()
            .map(|m| m.split(|&b| b == b'\n'))
            .into_iter()
            .flatten()
            .filter(|l| !l.is_empty())
    }

    /// 文件精确归属：返回拥有该文件的包名（去重）。
    pub fn owners_of(&self, target: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for line in self.lines() {
            let Some(tab) = memchr::memchr(b'\t', line) else {
                continue;
            };
            if line[..tab].trim_ascii_end() == target.as_bytes() {
                if let Ok(pkg) = std::str::from_utf8(&line[tab + 1..]) {
                    let pkg = pkg.trim();
                    if !pkg.is_empty() && !out.iter().any(|o| o == pkg) {
                        out.push(pkg.to_string());
                    }
                }
            }
        }
        out
    }

    /// `list --repo`：某包的全部文件。
    pub fn files_for_pkg(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in self.lines() {
            let Some(tab) = memchr::memchr(b'\t', line) else {
                continue;
            };
            if line[tab + 1..] == *name.as_bytes() {
                if let Ok(f) = std::str::from_utf8(&line[..tab]) {
                    out.push(f.to_string());
                }
            }
        }
        out
    }

    /// 路径谓词检索：返回 `(包名, 归一化绝对路径)`。
    pub fn search_paths<F: Fn(&str) -> bool>(&self, pred: F) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for line in self.lines() {
            let Some(tab) = memchr::memchr(b'\t', line) else {
                continue;
            };
            let (fb, pb) = line.split_at(tab);
            let Ok(f) = std::str::from_utf8(fb) else {
                continue;
            };
            let f_norm = if f.starts_with('/') {
                std::borrow::Cow::Borrowed(f)
            } else {
                std::borrow::Cow::Owned(format!("/{}", f))
            };
            if !pred(&f_norm) {
                continue;
            }
            if let Ok(pkg) = std::str::from_utf8(&pb[1..]) {
                let pkg = pkg.trim();
                if !pkg.is_empty() {
                    out.push((pkg.to_string(), f_norm.into_owned()));
                }
            }
        }
        out
    }
}

pub struct RpmBackend {
    local: Option<local::RpmLocal>,
    online: RpmOnline,
    repos: Vec<RepoConfig>,
    repo_packages_cache: OnceLock<Vec<PkgMetadata>>,
    /// metalink/mirrorlist 解析结果缓存（含 None，避免进程内重复拉取失败源）
    mirror_cache: Mutex<std::collections::HashMap<String, Option<String>>>,
    /// filelists 惰性加载缓存（P0-1/P1）；None 表示尚未尝试
    repo_files_cache: OnceLock<RpmFileIndex>,
    /// 最近一次全量刷新的分源统计（refresh_metadata 消费）
    last_refresh: std::sync::Mutex<Option<RefreshStats>>,
}

impl Default for RpmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RpmBackend {
    pub fn new() -> Self {
        let local = local::RpmLocal::load().ok();
        let online = RpmOnline::new();
        let repos = parse_repo_files()
            .into_iter()
            .filter(|r| {
                if !r.enabled {
                    return false;
                }
                // P0-5：metalink/mirrorlist 仓库不再被静默丢弃
                if r.baseurl.is_none() && r.mirrorlist.is_none() && r.metalink.is_none() {
                    tracing::warn!(
                        "Warning: repo '{}': no baseurl/mirrorlist/metalink configured, skipped",
                        r.id
                    );
                    return false;
                }
                true
            })
            .collect();

        Self {
            local,
            online,
            repos,
            repo_packages_cache: OnceLock::new(),
            last_refresh: std::sync::Mutex::new(None),
            mirror_cache: Mutex::new(std::collections::HashMap::new()),
            repo_files_cache: OnceLock::new(),
        }
    }

    /// 惰性加载仓库文件索引（P0-1/P1）。失败仓库静默跳过（owns 降级原则）。
    ///
    /// 先按源拉取小型 repomd 取得 filelists 时间戳作为失效键；键命中则直接 mmap
    /// 复用落盘的扁平索引，否则并行拉取解析 filelists、重建并落盘。
    fn ensure_repo_files(&self, cfg: &CacheConfig) -> &RpmFileIndex {
        use rayon::prelude::*;

        self.repo_files_cache.get_or_init(|| {
            let ttl = Duration::from_secs(cfg.ttl_secs);
            let vars = repo_parser::load_dnf_vars();

            // 阶段 1（串行）：解析 baseurl / 凭据，保留镜像告警顺序。
            let resolved: Vec<(&RepoConfig, String, Option<String>, Option<String>)> = self
                .repos
                .iter()
                .filter_map(|repo| {
                    let url = self.baseurl_for(repo, cfg)?;
                    let user = repo
                        .username
                        .as_ref()
                        .map(|u| repo_parser::expand_vars(u, &vars));
                    let pass = repo
                        .password
                        .as_ref()
                        .map(|p| repo_parser::expand_vars(p, &vars));
                    Some((repo, url, user, pass))
                })
                .collect();

            // 阶段 2（并行）：拉取 repomd，取 filelists 数据项的 (href, timestamp)。
            let metas: Vec<Option<(String, i64)>> = resolved
                .par_iter()
                .map(|item| {
                    let q = RepoQuery {
                        repo_id: &item.0.id,
                        base_url: &item.1,
                        ttl,
                        force: cfg.force_refresh,
                        offline: cfg.offline_mode,
                        username: item.2.as_deref(),
                        password: item.3.as_deref(),
                    };
                    self.online
                        .fetch_repomd_entry(q, "filelists")
                        .ok()
                        .flatten()
                })
                .collect();

            // 失效键：按仓库顺序拼接 filelists 时间戳。
            let key: String = resolved
                .iter()
                .zip(&metas)
                .map(|(item, meta)| match meta {
                    Some((_, ts)) => format!("{}:{}", item.0.id, ts),
                    None => format!("{}:x", item.0.id),
                })
                .collect::<Vec<_>>()
                .join("|");

            let cache_path = crate::cache::rpm_filelists_cache_path();
            let meta_path = crate::cache::rpm_filelists_meta_path();
            if let Some(meta) = crate::cache::RpmFilelistsMeta::load(&meta_path) {
                if meta.key == key {
                    if let Ok(file) = std::fs::File::open(&cache_path) {
                        // SAFETY: 只读映射 pkq 自身缓存文件，本进程不写入；
                        // Mmap 以 RAII 管理映射生命周期，无悬垂指针。
                        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
                            tracing::debug!("rpm filelists: flat index cache hit");
                            return RpmFileIndex { data: Some(mmap) };
                        }
                    }
                }
            }

            // 阶段 3（并行）：未命中则按源拉取并解析 filelists（失败降级为空）。
            let per_repo: Vec<std::collections::HashMap<String, Vec<String>>> = resolved
                .par_iter()
                .zip(&metas)
                .map(|(item, meta)| {
                    if meta.is_none() {
                        return std::collections::HashMap::new();
                    }
                    let q = RepoQuery {
                        repo_id: &item.0.id,
                        base_url: &item.1,
                        ttl,
                        force: cfg.force_refresh,
                        offline: cfg.offline_mode,
                        username: item.2.as_deref(),
                        password: item.3.as_deref(),
                    };
                    self.online
                        .fetch_and_parse_filelists(q)
                        .map(|(map, _)| map)
                        .unwrap_or_default()
                })
                .collect();

            // 阶段 4（串行）：构建扁平索引 `file\tpkg\n` 并原子落盘。
            let mut buf: Vec<u8> = Vec::new();
            for map in &per_repo {
                for (pkg, files) in map {
                    for f in files {
                        buf.extend_from_slice(f.as_bytes());
                        buf.push(b'\t');
                        buf.extend_from_slice(pkg.as_bytes());
                        buf.push(b'\n');
                    }
                }
            }
            if buf.is_empty() {
                // 无 filelists 数据：不写空缓存（避免 TTL 锁定空结果）。
                return RpmFileIndex::empty();
            }
            if let Some(parent) = cache_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("Failed to create cache dir {:?}: {}", parent, e);
                }
            }
            let tmp = cache_path.with_extension("tmp");
            if std::fs::write(&tmp, &buf).is_ok() {
                let _ = std::fs::rename(&tmp, &cache_path);
                let _ = crate::cache::RpmFilelistsMeta::save(&meta_path, key);
            }
            match std::fs::File::open(&cache_path) {
                // SAFETY: 只读映射刚写入的 pkq 缓存文件，本进程不再写入。
                Ok(file) => match unsafe { memmap2::Mmap::map(&file) } {
                    Ok(mmap) => RpmFileIndex { data: Some(mmap) },
                    Err(_) => RpmFileIndex::empty(),
                },
                Err(_) => RpmFileIndex::empty(),
            }
        })
    }

    /// 解析仓库可用 baseurl：优先 baseurl，其次惰性解析 metalink/mirrorlist（P0-5）
    fn baseurl_for(&self, repo: &RepoConfig, cfg: &CacheConfig) -> Option<String> {
        if let Some(b) = &repo.baseurl {
            return Some(resolve_url(b));
        }
        if let Ok(map) = self.mirror_cache.lock() {
            if let Some(v) = map.get(&repo.id) {
                return v.clone();
            }
        }
        let mirror_url = repo.metalink.as_ref().or(repo.mirrorlist.as_ref())?;
        let resolved = self.resolve_mirror(&repo.id, mirror_url, cfg);
        if let Ok(mut map) = self.mirror_cache.lock() {
            map.insert(repo.id.clone(), resolved.clone());
        }
        resolved
    }

    fn resolve_mirror(&self, repo_id: &str, mirror_url: &str, cfg: &CacheConfig) -> Option<String> {
        let url = resolve_url(mirror_url);
        if url.contains('$') {
            tracing::warn!(
                "Warning: repo '{}': mirror URL has unresolved variables: {}",
                repo_id,
                url
            );
            return None;
        }
        let mgr = crate::network::NetworkCacheManager::new();
        let ttl = Duration::from_secs(cfg.ttl_secs);
        let content = match mgr.fetch_text(FetchRequest {
            repo_id,
            url: &url,
            ttl,
            force: cfg.force_refresh,
            offline: cfg.offline_mode,
            username: None,
            password: None,
        }) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Warning: repo '{}': failed to fetch mirror list {}: {}",
                    repo_id,
                    url,
                    e
                );
                return None;
            }
        };
        let candidates = if repo_parser::looks_like_metalink_xml(&content) {
            repo_parser::parse_metalink_urls(&content)
        } else {
            repo_parser::parse_mirrorlist_lines(&content)
        };
        if candidates.is_empty() {
            tracing::warn!("Warning: repo '{}': no usable mirror in {}", repo_id, url);
            return None;
        }
        // 多镜像故障转移：按 preference 顺序探测候选的 repomd.xml 可达性，
        // 返回首个可用 baseurl（探测结果同时写入 pkq 缓存，主流程可直接命中）。
        for cand in &candidates {
            let base = repo_parser::baseurl_from_mirror_url(cand);
            let probe_url = format!("{}/repodata/repomd.xml", base);
            if mgr
                .fetch_index(FetchRequest {
                    repo_id,
                    url: &probe_url,
                    ttl,
                    force: cfg.force_refresh,
                    offline: cfg.offline_mode,
                    username: None,
                    password: None,
                })
                .is_ok()
            {
                return Some(base);
            }
        }
        // 全部探测失败：退化为首个候选，交由下游 stale/load_any 降级处理。
        Some(repo_parser::baseurl_from_mirror_url(&candidates[0]))
    }

    /// quiet=true 时完全静默（日常查询路径），false 时输出逐源刷新进度（cache update）。
    ///
    /// 仓库"拉取 + 解压 + XML 解析"属 I/O 与 CPU 混合密集，按源并行执行（对齐 DEB 侧
    /// `fetch_all_repos` 的 `rayon` 策略）。进度输出在收集后按**源顺序**统一打印，
    /// 保证 `cache update` 的行格式、`[i/N]` 编号与顺序和串行实现完全一致。
    fn get_repo_packages_ref(&self, cfg: &CacheConfig, quiet: bool) -> Result<&[PkgMetadata]> {
        use rayon::prelude::*;

        if let Some(cached) = self.repo_packages_cache.get() {
            return Ok(cached.as_slice());
        }
        let ttl = Duration::from_secs(cfg.ttl_secs);
        let vars = repo_parser::load_dnf_vars();
        let total_repos = self.repos.len();

        enum Resolved {
            Skipped,
            InvalidVar,
            Ready {
                url: String,
                user: Option<String>,
                pass: Option<String>,
            },
        }

        // 阶段 1（串行）：解析各源 baseurl / 凭据。镜像相关告警仍按源顺序打印。
        let resolved: Vec<(usize, &RepoConfig, Resolved)> = self
            .repos
            .iter()
            .enumerate()
            .map(|(idx, repo)| {
                let url = match self.baseurl_for(repo, cfg) {
                    Some(u) => u,
                    None => return (idx, repo, Resolved::Skipped),
                };
                if url.contains('$') {
                    return (idx, repo, Resolved::InvalidVar);
                }
                let user = repo
                    .username
                    .as_ref()
                    .map(|u| repo_parser::expand_vars(u, &vars));
                let pass = repo
                    .password
                    .as_ref()
                    .map(|p| repo_parser::expand_vars(p, &vars));
                (idx, repo, Resolved::Ready { url, user, pass })
            })
            .collect();

        if !quiet && total_repos > 0 {
            eprintln!("正在刷新仓库元数据（共 {} 个源）...", total_repos);
        }

        enum Fetched {
            Skipped,
            InvalidVar,
            Online(Vec<PkgMetadata>),
            Fallback(Vec<PkgMetadata>, String),
            Failed(String),
        }

        // 阶段 2（并行）：按源拉取并解析 primary 元数据；此阶段不打印，避免并发交错。
        let fetched: Vec<Fetched> = resolved
            .par_iter()
            .map(|(_, repo, state)| match state {
                Resolved::Skipped => Fetched::Skipped,
                Resolved::InvalidVar => Fetched::InvalidVar,
                Resolved::Ready { url, user, pass } => {
                    let q = RepoQuery {
                        repo_id: &repo.id,
                        base_url: url,
                        ttl,
                        force: cfg.force_refresh,
                        offline: cfg.offline_mode,
                        username: user.as_deref(),
                        password: pass.as_deref(),
                    };
                    match self.fetch_repo_packages_cached(q) {
                        Ok((packages, FetchOutcome::Online)) => Fetched::Online(packages),
                        Ok((packages, FetchOutcome::LocalFallback(reason))) => {
                            Fetched::Fallback(packages, reason)
                        }
                        Err(e) => Fetched::Failed(crate::backend::common::brief_network_reason(
                            &e.to_string(),
                        )),
                    }
                }
            })
            .collect();

        // 阶段 3（串行）：按源顺序渲染进度并汇总。
        let mut all = Vec::new();
        let (mut n_online, mut n_fb, mut n_fail) = (0usize, 0usize, 0usize);
        for ((idx, repo, _), outcome) in resolved.iter().zip(fetched) {
            match outcome {
                Fetched::Skipped => {}
                Fetched::InvalidVar => {
                    n_fail += 1;
                    if !quiet {
                        eprintln!(
                            "  ✗ [{:>2}/{}] {} —— baseurl 含未解析变量",
                            idx + 1,
                            total_repos,
                            repo.id
                        );
                    }
                }
                Fetched::Online(packages) => {
                    n_online += 1;
                    if !quiet {
                        eprintln!("  ✓ [{:>2}/{}] {}", idx + 1, total_repos, repo.id);
                    }
                    all.extend(packages);
                }
                Fetched::Fallback(packages, reason) => {
                    n_fb += 1;
                    if !quiet {
                        eprintln!(
                            "  △ [{:>2}/{}] {} —— {}，已回退本地缓存",
                            idx + 1,
                            total_repos,
                            repo.id,
                            reason
                        );
                    }
                    all.extend(packages);
                }
                Fetched::Failed(reason) => {
                    n_fail += 1;
                    if !quiet {
                        eprintln!(
                            "  ✗ [{:>2}/{}] {} —— {}",
                            idx + 1,
                            total_repos,
                            repo.id,
                            reason
                        );
                    }
                }
            }
        }
        if !quiet && total_repos > 0 {
            eprintln!(
                "  源刷新完成：在线成功 {} 个，回退本地 {} 个，失败 {} 个",
                n_online, n_fb, n_fail
            );
        }
        *crate::backend::common::lock_recover(&self.last_refresh) = Some(RefreshStats {
            total: total_repos,
            online: n_online,
            fallback: n_fb,
            failed: n_fail,
        });
        let _ = self.repo_packages_cache.set(all);
        Ok(self.repo_packages_cache.get().unwrap().as_slice())
    }

    /// 仓库 primary 包数据：解析结果磁盘缓存（P1-1），
    /// 失效键 = repomd 中 primary 数据项 timestamp（repomd 本身走 ETag/TTL，代价极小）
    fn fetch_repo_packages_cached(
        &self,
        q: RepoQuery<'_>,
    ) -> Result<(Vec<PkgMetadata>, FetchOutcome)> {
        let cache_path = cache::rpm_repo_cache_path(q.repo_id, q.base_url);
        match self.online.fetch_repomd_meta(q) {
            Ok(meta) => {
                if !q.force {
                    if let Some(cached) = cache::RpmRepoCache::load(&cache_path, meta.primary_ts) {
                        let mut packages = cached.packages;
                        // 旧版本缓存兼容：补填 source_repo；清理历史遗留的 epoch="0"
                        for p in &mut packages {
                            if p.source_repo.is_none() {
                                p.source_repo = Some(q.repo_id.to_string());
                            }
                            if p.epoch.as_deref() == Some("0") {
                                p.epoch = None;
                            }
                        }
                        return Ok((packages, FetchOutcome::Online));
                    }
                }
                let packages = self.online.fetch_primary_packages(q, &meta.primary_href)?;
                let _ = cache::RpmRepoCache::save(&cache_path, meta.primary_ts, packages.clone());
                Ok((packages, FetchOutcome::Online))
            }
            Err(_) => {
                // repomd 不可用（源故障/离线）：降级加载本地解析缓存，
                // 数据可能过期但仓库查询不因单点 repomd 故障而全空
                match cache::RpmRepoCache::load_any(&cache_path) {
                    Some(c) => Ok((
                        c.packages,
                        FetchOutcome::LocalFallback("repomd 不可达".into()),
                    )),
                    None => Err(PkgError::NetworkError(format!(
                        "repomd unavailable for {}",
                        q.repo_id
                    ))),
                }
            }
        }
    }

    fn require_local(&self) -> Result<&local::RpmLocal> {
        self.local.as_ref().ok_or_else(|| {
            PkgError::DatabaseError(
                "RPM local database not available. Use --repo for online queries.".into(),
            )
        })
    }

    fn get_repo_changelog_map(
        &self,
        cfg: &CacheConfig,
    ) -> Result<std::collections::HashMap<String, Vec<ChangelogEntry>>> {
        let ttl = Duration::from_secs(cfg.ttl_secs);
        let vars = repo_parser::load_dnf_vars();
        use rayon::prelude::*;

        // 阶段 1（串行）：解析 baseurl / 凭据，保留镜像告警顺序。
        let resolved: Vec<(&RepoConfig, String, Option<String>, Option<String>)> = self
            .repos
            .iter()
            .filter_map(|repo| {
                let url = self.baseurl_for(repo, cfg)?;
                let user = repo
                    .username
                    .as_ref()
                    .map(|u| repo_parser::expand_vars(u, &vars));
                let pass = repo
                    .password
                    .as_ref()
                    .map(|p| repo_parser::expand_vars(p, &vars));
                Some((repo, url, user, pass))
            })
            .collect();

        // 阶段 2（并行）：按源拉取 other.xml；阶段 3 按源顺序合并并打印告警。
        type ChangelogMap = std::collections::HashMap<String, Vec<ChangelogEntry>>;
        let per_repo: Vec<(String, Result<ChangelogMap>)> = resolved
            .par_iter()
            .map(|(repo, url, user, pass)| {
                let q = RepoQuery {
                    repo_id: &repo.id,
                    base_url: url,
                    ttl,
                    force: cfg.force_refresh,
                    offline: cfg.offline_mode,
                    username: user.as_deref(),
                    password: pass.as_deref(),
                };
                let res = self.online.fetch_and_parse_other(q);
                (repo.id.clone(), res)
            })
            .collect();

        let mut all_map: std::collections::HashMap<String, Vec<ChangelogEntry>> =
            std::collections::HashMap::new();
        for (repo_id, res) in per_repo {
            match res {
                Ok(map) => {
                    for (k, v) in map {
                        all_map.entry(k).or_default().extend(v);
                    }
                }
                Err(e) => tracing::warn!("Warning: repo '{}': {}", repo_id, e),
            }
        }
        Ok(all_map)
    }
}

impl PkgBackend for RpmBackend {
    fn system_type(&self) -> PackageSystem {
        PackageSystem::Rpm
    }

    fn refresh_metadata(&self, cfg: &CacheConfig) -> Result<RefreshReport> {
        let force_cfg = CacheConfig {
            ttl_secs: 0,
            force_refresh: true,
            offline_mode: cfg.offline_mode,
        };
        let packages = self.get_repo_packages_ref(&force_cfg, false)?;
        let stats = crate::backend::common::lock_recover(&self.last_refresh).unwrap_or_default();
        Ok(RefreshReport {
            stats,
            package_count: packages.len(),
        })
    }

    fn resolve_dep_name(&self, dep_name: &str) -> Option<String> {
        self.local
            .as_ref()
            .and_then(|l| l.resolve_dep_to_pkg(dep_name))
    }

    fn get_package_details(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Option<PkgMetadata>> {
        match source {
            PackageSource::Installed => Ok(self.require_local()?.find_by_name(name).cloned()),
            PackageSource::Repo => Ok(self
                .get_repo_packages_ref(cfg, true)?
                .iter()
                .find(|p| p.name == name)
                .cloned()),
        }
    }

    fn list_files(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<String>> {
        match source {
            PackageSource::Installed => {
                let local = self.require_local()?;
                Ok(local.get_package_files(name))
            }
            PackageSource::Repo => {
                // 优先 filelists 反查（primary.xml 不含文件数据，P0-1/P1）
                let repo_files = self.ensure_repo_files(cfg);
                let files = repo_files.files_for_pkg(name);
                if !files.is_empty() {
                    return Ok(files);
                }
                let packages = self.get_repo_packages_ref(cfg, true)?;
                Ok(packages
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.files.clone())
                    .unwrap_or_default())
            }
        }
    }

    fn find_file_owner(
        &self,
        file_path: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<String>> {
        match source {
            PackageSource::Installed => Ok(self.require_local()?.find_file_owner(file_path)),
            PackageSource::Repo => {
                // filelists 驱动（P0-1/P1）
                let repo_files = self.ensure_repo_files(cfg);
                let target = file_path.trim_end_matches('/');
                Ok(repo_files.owners_of(target))
            }
        }
    }

    fn get_dependencies(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<Dependency>> {
        match source {
            PackageSource::Installed => {
                let local = self.require_local()?;
                Ok(local.resolve_deps_to_packages(name))
            }
            PackageSource::Repo => {
                let packages = self.get_repo_packages_ref(cfg, true)?;
                let pkg = match packages.iter().find(|p| p.name == name) {
                    Some(p) => p,
                    None => return Ok(Vec::new()),
                };
                let local = self.local.as_ref();
                let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
                let mut result = Vec::new();
                for req in &pkg.requires {
                    let resolved = if let Some(l) = local {
                        l.resolve_dep_to_pkg(&req.name)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| req.name.clone());
                    if resolved == name {
                        continue;
                    }
                    if resolved.contains('/') || !resolved.chars().any(|c| c.is_ascii_alphabetic())
                    {
                        continue;
                    }
                    if !seen.insert(resolved.clone()) {
                        continue;
                    }
                    result.push(Dependency {
                        name: resolved,
                        version: req.version.clone(),
                        flags: req.flags.clone(),
                        is_alternative: req.is_alternative,
                    });
                }
                Ok(result)
            }
        }
    }

    fn get_reverse_dependencies(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<ReverseDep>> {
        let local = self.local.as_ref();

        let target_caps = if let Some(l) = local {
            l.build_target_caps(name)
        } else {
            let mut caps = std::collections::HashSet::new();
            caps.insert(name.to_string());
            caps
        };

        let fmt_version = |p: &PkgMetadata| -> String {
            if !p.release.is_empty() {
                format!("{}-{}", p.version, p.release)
            } else {
                p.version.clone()
            }
        };

        match source {
            PackageSource::Installed => {
                let local_db = self.require_local()?;
                Ok(local_db
                    .packages
                    .iter()
                    .filter_map(|p| {
                        if p.name == name {
                            return None;
                        }
                        let matches =
                            crate::backend::rpm::local::check_pkg_requires(p, &target_caps);
                        if matches.is_empty() {
                            None
                        } else {
                            Some(ReverseDep {
                                pkg_name: p.name.clone(),
                                version: fmt_version(p),
                                arch: Some(p.arch.clone()),
                                source_repo: None,
                                matched_deps: matches,
                            })
                        }
                    })
                    .collect())
            }
            PackageSource::Repo => {
                let repo_pkgs = self.get_repo_packages_ref(cfg, true)?;
                Ok(repo_pkgs
                    .iter()
                    .filter_map(|p| {
                        if p.name == name {
                            return None;
                        }
                        let matches =
                            crate::backend::rpm::local::check_pkg_requires(p, &target_caps);
                        if matches.is_empty() {
                            None
                        } else {
                            Some(ReverseDep {
                                pkg_name: p.name.clone(),
                                version: fmt_version(p),
                                arch: Some(p.arch.clone()),
                                // 精确 NVR.A 判定：同名不同版本的仓库包 ≠ 已安装
                                source_repo: if self
                                    .local
                                    .as_ref()
                                    .map(|l| {
                                        l.is_installed_exact(
                                            &p.name, &p.version, &p.release, &p.arch,
                                        )
                                    })
                                    .unwrap_or(false)
                                {
                                    None
                                } else {
                                    p.source_repo.clone()
                                },
                                matched_deps: matches,
                            })
                        }
                    })
                    .collect())
            }
        }
    }

    fn search_packages(
        &self,
        keyword: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<PkgMetadata>> {
        match source {
            PackageSource::Installed => Ok(self
                .require_local()?
                .search(keyword)
                .into_iter()
                .cloned()
                .collect()),
            PackageSource::Repo => {
                let packages = self.get_repo_packages_ref(cfg, true)?;
                let kw = keyword.to_lowercase();
                Ok(packages
                    .iter()
                    .filter(|p| {
                        p.name.to_lowercase().contains(&kw)
                            || p.summary.to_lowercase().contains(&kw)
                            || p.description.to_lowercase().contains(&kw)
                    })
                    .cloned()
                    .collect())
            }
        }
    }

    fn search_by_file(&self, file_path: &str, cfg: &CacheConfig) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        if let Ok(local) = self.require_local() {
            let owners = local.find_file_owner(file_path);
            for owner in owners {
                results.push(SearchResult {
                    pkg_name: owner,
                    matched_text: file_path.to_string(),
                    match_type: "file".to_string(),
                    source: "installed".to_string(),
                });
            }
        }

        let repo_files = self.ensure_repo_files(cfg);
        let target = file_path.trim_end_matches('/');
        for pkg in repo_files.owners_of(target) {
            let already = results.iter().any(|r| r.pkg_name == pkg);
            if !already {
                results.push(SearchResult {
                    pkg_name: pkg,
                    matched_text: file_path.to_string(),
                    match_type: "file".to_string(),
                    source: "repo".to_string(),
                });
            }
        }

        Ok(results)
    }

    /// 双端对齐（与 DebBackend 一致）：无论 source 参数如何，始终基于仓库全量
    /// 元数据聚合源码包视图，本地库仅用于比对安装状态
    fn get_source_package(
        &self,
        name: &str,
        _source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Option<SourcePackageInfo>> {
        let all_packages = self.get_repo_packages_ref(cfg, true)?;

        // 解析 srpm 文件名（bash-5.2.15-19.src.rpm → "bash"）的公共闭包
        let src_name_of = |p: &PkgMetadata| -> Option<String> {
            p.source_pkg.as_ref().map(|src| {
                let base = src.trim_end_matches(".src.rpm");
                base.rsplitn(3, '-').nth(2).unwrap_or(base).to_string()
            })
        };

        // Step 1: 以二进制包名反查源码包
        let source_name = if let Some(pkg) = all_packages.iter().find(|p| p.name == name) {
            src_name_of(pkg).unwrap_or_else(|| name.to_string())
        } else if let Some(pkg) = self.local.as_ref().and_then(|l| l.find_by_name(name)) {
            src_name_of(pkg).unwrap_or_else(|| name.to_string())
        } else {
            name.to_string()
        };

        // Step 2: 聚合属于该源码包的全部二进制包
        let mut binaries: Vec<BinaryPackageInfo> = all_packages
            .iter()
            .filter(|p| src_name_of(p).as_deref() == Some(source_name.as_str()))
            .map(|p| BinaryPackageInfo {
                name: p.name.clone(),
                version: if !p.release.is_empty() {
                    format!("{}-{}", p.version, p.release)
                } else {
                    p.version.clone()
                },
                arch: p.arch.clone(),
                // source 全景视图按包名判定安装状态：本地存在同名包即视为
                // 已安装（版本差异属于“仓库有更新”，由版本号列体现）
                installed: self
                    .local
                    .as_ref()
                    .map(|l| l.is_installed_by_name(&p.name))
                    .unwrap_or(false),
                source_repo: p.source_repo.clone(),
            })
            .collect();

        if binaries.is_empty() {
            return Ok(None);
        }

        // 与 DEB 对齐：去重 + 主同名包置顶 > 已安装优先 > 主架构 > 字母序
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        binaries.retain(|b| seen.insert((b.name.clone(), b.arch.clone())));
        binaries.sort_by(|a, b| {
            let a_main = a.name == source_name;
            let b_main = b.name == source_name;
            b_main
                .cmp(&a_main)
                .then_with(|| b.installed.cmp(&a.installed))
                .then_with(|| {
                    crate::backend::common::arch_weight(&a.arch)
                        .cmp(&crate::backend::common::arch_weight(&b.arch))
                })
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| b.version.cmp(&a.version))
        });

        let representative = all_packages
            .iter()
            .find(|p| p.name == source_name)
            .or_else(|| {
                all_packages
                    .iter()
                    .find(|p| src_name_of(p).as_deref() == Some(source_name.as_str()))
            });

        // 头部版本优先取本地已安装版本（与 info 命令一致），仓库版本次之
        let local_version = self
            .local
            .as_ref()
            .and_then(|l| l.find_by_name(&source_name))
            .map(|p| {
                if !p.release.is_empty() {
                    format!("{}-{}", p.version, p.release)
                } else {
                    p.version.clone()
                }
            });
        Ok(Some(SourcePackageInfo {
            name: source_name,
            version: local_version.unwrap_or_else(|| {
                binaries
                    .first()
                    .map(|b| b.version.clone())
                    .unwrap_or_default()
            }),
            release: String::new(),
            url: representative.and_then(|p| p.url.clone()),
            license: representative.and_then(|p| p.license.clone()),
            maintainer: representative.and_then(|p| p.packager.clone()),
            binaries,
        }))
    }

    fn get_changelog(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<ChangelogEntry>> {
        match source {
            PackageSource::Installed => {
                let local = self.require_local()?;
                match local.find_by_name(name) {
                    Some(pkg) => Ok(pkg.changelog.clone()),
                    None => Ok(Vec::new()),
                }
            }
            PackageSource::Repo => {
                let changelog_map = self.get_repo_changelog_map(cfg)?;
                Ok(changelog_map.get(name).cloned().unwrap_or_default())
            }
        }
    }

    fn search_by_pattern(
        &self,
        pattern: &str,
        use_regex: bool,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<SearchResult>> {
        let re = if use_regex {
            Some(
                regex::Regex::new(pattern)
                    .map_err(|e| PkgError::InvalidArgument(format!("无效的正则表达式: {}", e)))?,
            )
        } else {
            None
        };

        let match_fn = |text: &str| -> bool {
            if let Some(ref re) = re {
                re.is_match(text)
            } else {
                text.to_lowercase().contains(&pattern.to_lowercase())
            }
        };

        let pattern_lower = pattern.to_lowercase();
        let file_match_fn = |f_norm: &str| -> bool {
            if use_regex {
                return match_fn(f_norm);
            }
            if crate::backend::common::has_noise_extension(&f_norm.to_lowercase()) {
                return false;
            }
            crate::backend::common::path_segment_match(f_norm, &pattern_lower)
        };

        let is_file_path = pattern.starts_with('/');
        let mut results = Vec::new();

        // 1. keyword search（借用切片，避免整库深拷贝）
        if !is_file_path || use_regex {
            let local_pkgs;
            let repo_pkgs;
            let packages: &[PkgMetadata] = match source {
                PackageSource::Installed => {
                    local_pkgs = &self.require_local()?.packages;
                    local_pkgs
                }
                PackageSource::Repo => {
                    repo_pkgs = self.get_repo_packages_ref(cfg, true)?;
                    repo_pkgs
                }
            };
            for pkg in packages {
                if match_fn(&pkg.name) || match_fn(&pkg.summary) {
                    results.push(SearchResult {
                        pkg_name: pkg.name.clone(),
                        matched_text: pkg.summary.clone(),
                        match_type: "keyword".to_string(),
                        source: match source {
                            PackageSource::Installed => "installed",
                            PackageSource::Repo => "repo",
                        }
                        .to_string(),
                    });
                }
            }
        }

        // 2. file search — 本地直接使用 rpmdb 解析出的文件表（P1-3：不再调用 rpm 子进程），
        //    仓库走 filelists（P0-1）
        match source {
            PackageSource::Installed => {
                let local = self.require_local()?;
                for pkg in &local.packages {
                    for f in &pkg.files {
                        let f_norm = if f.starts_with('/') {
                            f.clone()
                        } else {
                            format!("/{}", f)
                        };
                        if file_match_fn(&f_norm) {
                            results.push(SearchResult {
                                pkg_name: pkg.name.clone(),
                                matched_text: f_norm,
                                match_type: "file".to_string(),
                                source: "installed".to_string(),
                            });
                        }
                    }
                }
            }
            PackageSource::Repo => {
                let repo_files = self.ensure_repo_files(cfg);
                for (pkg, f_norm) in repo_files.search_paths(|f| file_match_fn(f)) {
                    results.push(SearchResult {
                        pkg_name: pkg,
                        matched_text: f_norm,
                        match_type: "file".to_string(),
                        source: "repo".to_string(),
                    });
                }
            }
        }

        // Provides: only absolute-path provides (starting with /) are treated as file matches;
        // virtual provides like perl(...), npm(...) are excluded.
        let local_pkgs;
        let repo_pkgs;
        let packages: &[PkgMetadata] = match source {
            PackageSource::Installed => {
                local_pkgs = &self.require_local()?.packages;
                local_pkgs
            }
            PackageSource::Repo => {
                repo_pkgs = self.get_repo_packages_ref(cfg, true)?;
                repo_pkgs
            }
        };
        for pkg in packages {
            for prov in &pkg.provides {
                if !prov.starts_with('/') {
                    continue;
                }
                let f_norm = prov.clone();
                if file_match_fn(&f_norm) {
                    results.push(SearchResult {
                        pkg_name: pkg.name.clone(),
                        matched_text: f_norm,
                        match_type: "file".to_string(),
                        source: match source {
                            PackageSource::Installed => "installed",
                            PackageSource::Repo => "repo",
                        }
                        .to_string(),
                    });
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl RpmFileIndex {
        /// 测试专用：从字节缓冲构建（写临时文件后 mmap，与生产路径同构）。
        fn from_bytes(buf: &[u8]) -> Self {
            let dir = std::env::temp_dir().join("pkq-test-fileindex");
            std::fs::create_dir_all(&dir).ok();
            let path = dir.join(format!("idx-{}.txt", std::process::id()));
            std::fs::write(&path, buf).expect("write test index");
            let file = std::fs::File::open(&path).expect("open test index");
            // SAFETY: 只读映射测试用临时文件，进程内不写入；RAII 管理生命周期。
            let mmap = unsafe { memmap2::Mmap::map(&file).expect("mmap test index") };
            Self { data: Some(mmap) }
        }
    }

    #[test]
    fn rpm_file_index_owners_files_search() {
        let buf = b"/usr/bin/unzip\tunzip\n\
                    /usr/share/man/man1/unzip.1.gz\tunzip\n\
                    /usr/bin/zip\tzip\n\
                    usr/rel/abs\tb\n";
        let idx = RpmFileIndex::from_bytes(buf);

        // 精确归属
        assert_eq!(idx.owners_of("/usr/bin/unzip"), vec!["unzip".to_string()]);
        assert!(idx.owners_of("/nope").is_empty());
        // list --repo：包名 → 文件
        assert_eq!(idx.files_for_pkg("zip"), vec!["/usr/bin/zip".to_string()]);
        assert!(idx.files_for_pkg("missing").is_empty());
        // 路径检索：相对路径归一为绝对路径
        let hits = idx.search_paths(|f| f == "/usr/rel/abs");
        assert_eq!(hits, vec![("b".to_string(), "/usr/rel/abs".to_string())]);
        // 谓词检索（前缀）
        let man = idx.search_paths(|f| f.starts_with("/usr/share/man/"));
        assert_eq!(man.len(), 1);
        assert_eq!(man[0].0, "unzip");
    }
}

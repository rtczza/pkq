pub mod local;
pub mod online;

use std::sync::OnceLock;
use std::time::Duration;

use rayon::prelude::*;

use crate::backend::common::RefreshReport;
use crate::backend::PkgBackend;
use crate::cache;
use crate::error::{PkgError, Result};
use crate::model::*;
use online::{parse_apt_sources, AptSource, DebOnline};

static REPO_CACHE: OnceLock<Vec<PkgMetadata>> = OnceLock::new();

pub struct DebBackend {
    local: Option<local::DebLocal>,
    online: DebOnline,
    sources: Vec<AptSource>,
}

impl Default for DebBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DebBackend {
    pub fn new() -> Self {
        let local = local::DebLocal::load().ok();
        let online = DebOnline::new();
        let sources = parse_apt_sources();
        Self {
            local,
            online,
            sources,
        }
    }

    /// quiet=true 时完全静默（日常查询路径），false 时输出逐源刷新进度（cache update）
    fn get_repo_packages_ref(
        &self,
        cfg: &CacheConfig,
        quiet: bool,
    ) -> Result<&'static [PkgMetadata]> {
        if let Some(cached) = REPO_CACHE.get() {
            return Ok(cached.as_slice());
        }

        let cache_path = cache::deb_cache_path();
        let source_files = cache::deb_apt_lists_packages();

        if let Some(cached) =
            cache::PkgIndexCache::load(&cache_path, cfg.ttl_secs, cfg.force_refresh, &source_files)
        {
            let _ = REPO_CACHE.set(cached.packages);
            return Ok(REPO_CACHE.get().unwrap().as_slice());
        }

        let ttl = Duration::from_secs(cfg.ttl_secs);
        let summary = self.online.fetch_all_repos(
            &self.sources,
            ttl,
            cfg.force_refresh,
            cfg.offline_mode,
            quiet,
        );
        let _ = cache::PkgIndexCache::save(&cache_path, summary.packages.clone());
        let _ = REPO_CACHE.set(summary.packages);
        Ok(REPO_CACHE.get().unwrap().as_slice())
    }

    fn require_local(&self) -> Result<&local::DebLocal> {
        self.local
            .as_ref()
            .ok_or_else(|| PkgError::DatabaseError("DEB local database not available".into()))
    }

    fn installed_version_arch(&self, name: &str, arch: &str) -> Option<String> {
        self.local
            .as_ref()
            .and_then(|l| l.find_by_name_arch(name, arch))
            .map(|p| p.version.clone())
    }

    /// 确保原始 Contents 缓存存在且有效（list --repo 与 search 共用）。
    /// 无本地 Contents 数据时不写空缓存（P1-5：避免空结果被 TTL 锁定）。
    fn ensure_contents_raw(&self, arch: &str, cfg: &CacheConfig) {
        let raw_path = cache::deb_contents_raw_cache_path();
        let meta_path = cache::deb_contents_raw_meta_path();
        let source_files: Vec<_> = std::fs::read_dir("/var/lib/apt/lists")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.to_string_lossy();
                name.contains("Contents-") && name.ends_with(".lz4")
            })
            .collect();

        if !cache::ContentsMeta::check_valid(
            &meta_path,
            cfg.ttl_secs,
            cfg.force_refresh,
            &source_files,
        ) {
            let raw = self.online.collect_all_contents_raw(arch);
            if raw.is_empty() {
                return;
            }
            let tmp = raw_path.with_extension("tmp");
            let _ = std::fs::write(&tmp, &raw);
            let _ = std::fs::rename(&tmp, &raw_path);
            let _ = cache::ContentsMeta::save_meta(&meta_path);
        }
    }

    /// Contents 反查：收集仓库中某包的全部文件（P3-6 提前实现，全量扫描，阶段 2 索引化）
    fn list_files_from_contents(&self, name: &str, arch: &str, cfg: &CacheConfig) -> Vec<String> {
        self.ensure_contents_raw(arch, cfg);
        let raw_path = cache::deb_contents_raw_cache_path();
        let file = match std::fs::File::open(&raw_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        // SAFETY: mmap 映射只读文件（Contents 缓存，本进程只读不写）；
        // 映射生命周期由 Mmap RAII 管理，无悬垂指针；失败时返回空结果。
        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let target = name;
        let mut files: Vec<String> = Vec::new();
        for line in mmap.split(|&b| b == b'\n') {
            if line.is_empty() || line.starts_with(b"FILE") {
                continue;
            }
            let tab = match memchr::memchr(b'\t', line) {
                Some(p) => p,
                None => continue,
            };
            let (fp, pkg_field) = line.split_at(tab);
            let pkg_field = &pkg_field[1..];
            let hit = std::str::from_utf8(pkg_field)
                .map(|s| {
                    s.split(',').any(|e| {
                        e.split('/')
                            .next_back()
                            .map(|n| n.trim() == target)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if !hit {
                continue;
            }
            let path = if fp.first() == Some(&b'/') {
                String::from_utf8_lossy(fp).into_owned()
            } else {
                format!("/{}", String::from_utf8_lossy(fp))
            };
            files.push(path);
        }
        files.sort();
        files.dedup();
        files
    }

    fn search_contents_cached(
        &self,
        pattern: &str,
        use_regex: bool,
        arch: &str,
        cfg: &CacheConfig,
    ) -> Vec<SearchResult> {
        self.ensure_contents_raw(arch, cfg);

        let raw_path = cache::deb_contents_raw_cache_path();
        let file = match std::fs::File::open(&raw_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        // SAFETY: mmap 映射只读文件（Contents 缓存，本进程只读不写）；
        // 映射生命周期由 Mmap RAII 管理，无悬垂指针；失败时返回空结果。
        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let pattern_lower = pattern.to_lowercase();
        let pattern_bytes = pattern_lower.as_bytes();
        let re = if use_regex {
            match regex::Regex::new(pattern) {
                Ok(r) => Some(r),
                Err(_) => return Vec::new(),
            }
        } else {
            None
        };

        let chunk_size = 8 * 1024 * 1024;
        let chunks = line_aligned_chunks(&mmap, chunk_size);

        let results: Vec<Vec<SearchResult>> = chunks
            .par_iter()
            .map(|chunk| {
                let mut local_results = Vec::new();
                let mut pos = 0;
                while pos < chunk.len() {
                    let line_end = memchr::memchr(b'\n', &chunk[pos..])
                        .map(|i| pos + i)
                        .unwrap_or(chunk.len());
                    let line = &chunk[pos..line_end];
                    pos = if line_end < chunk.len() {
                        line_end + 1
                    } else {
                        chunk.len()
                    };

                    if line.is_empty() || line.starts_with(b"FILE") {
                        continue;
                    }

                    if !use_regex && memchr::memmem::find(line, pattern_bytes).is_none() {
                        continue;
                    }

                    let tab_pos = match memchr::memchr(b'\t', line) {
                        Some(p) => p,
                        None => continue,
                    };

                    let file_path_bytes = &line[..tab_pos];
                    let pkg_field_bytes = &line[tab_pos + 1..];

                    if !use_regex && crate::backend::common::is_noise_ext_bytes(file_path_bytes) {
                        continue;
                    }

                    let search_path = if file_path_bytes.first() == Some(&b'/') {
                        String::from_utf8_lossy(file_path_bytes).into_owned()
                    } else {
                        let mut s = String::with_capacity(file_path_bytes.len() + 1);
                        s.push('/');
                        s.push_str(&String::from_utf8_lossy(file_path_bytes));
                        s
                    };

                    let matched = if let Some(ref re) = re {
                        re.is_match(&search_path)
                    } else {
                        crate::backend::common::path_segment_match(&search_path, &pattern_lower)
                    };

                    if matched {
                        if let Ok(pkg_field) = std::str::from_utf8(pkg_field_bytes) {
                            for pkg_entry in pkg_field.split(',') {
                                let pkg_name =
                                    pkg_entry.split('/').next_back().unwrap_or(pkg_entry).trim();
                                if !pkg_name.is_empty() {
                                    local_results.push(SearchResult {
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
                local_results
            })
            .collect();

        let mut all_results: Vec<SearchResult> = Vec::new();
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for batch in results {
            for r in batch {
                let key = (r.pkg_name.clone(), r.matched_text.clone());
                if seen.insert(key) {
                    all_results.push(r);
                }
            }
        }

        all_results
    }

    fn dedup_and_sort_binaries(
        &self,
        pkgs: Vec<BinaryPackageInfo>,
        source_name: &str,
    ) -> Vec<BinaryPackageInfo> {
        let mut groups: std::collections::HashMap<(String, String), BinaryPackageInfo> =
            std::collections::HashMap::new();
        for b in pkgs {
            let key = (b.name.clone(), b.arch.clone());
            match groups.get(&key) {
                None => {
                    groups.insert(key, b);
                }
                Some(existing) => {
                    if !existing.installed && (b.installed || b.version > existing.version) {
                        groups.insert(key, b);
                    }
                }
            }
        }
        let mut result: Vec<BinaryPackageInfo> = groups.into_values().collect();
        result.sort_by(|a, b| {
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
        result
    }
}

fn line_aligned_chunks(data: &[u8], target_size: usize) -> Vec<&[u8]> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < data.len() {
        if start + target_size >= data.len() {
            chunks.push(&data[start..]);
            break;
        }
        let nl = memchr::memchr(b'\n', &data[start + target_size..])
            .map(|i| start + target_size + i + 1)
            .unwrap_or(data.len());
        chunks.push(&data[start..nl]);
        start = nl;
    }
    chunks
}

impl PkgBackend for DebBackend {
    fn system_type(&self) -> PackageSystem {
        PackageSystem::Deb
    }

    fn refresh_metadata(&self, cfg: &CacheConfig) -> Result<RefreshReport> {
        // 独立刷新路径：绕过进程内 OnceLock（保证 force 语义），
        // ttl=0 + force 使 network 层绕过 ETag/TTL 全量重拉
        let summary = self.online.fetch_all_repos(
            &self.sources,
            Duration::ZERO,
            true,
            cfg.offline_mode,
            false,
        );
        let cache_path = cache::deb_cache_path();
        let _ = cache::PkgIndexCache::save(&cache_path, summary.packages.clone());
        let _ = REPO_CACHE.set(summary.packages);
        Ok(RefreshReport {
            stats: summary.stats,
            package_count: summary.package_count,
        })
    }

    fn get_package_details(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Option<PkgMetadata>> {
        match source {
            PackageSource::Installed => Ok(self.require_local()?.find_by_name(name).cloned()),
            PackageSource::Repo => {
                let packages = self.get_repo_packages_ref(cfg, true)?;
                Ok(packages.iter().find(|p| p.name == name).cloned())
            }
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
                // Contents 反查（P0-1 DEB 侧）
                let arch = self
                    .sources
                    .first()
                    .map(|s| s.arch.as_str())
                    .unwrap_or("amd64");
                Ok(self.list_files_from_contents(name, arch, cfg))
            }
        }
    }

    fn find_file_owner(
        &self,
        file_path: &str,
        source: PackageSource,
        _cfg: &CacheConfig,
    ) -> Result<Vec<String>> {
        match source {
            PackageSource::Installed => Ok(self.require_local()?.find_file_owner(file_path)),
            PackageSource::Repo => {
                let arch = self
                    .sources
                    .first()
                    .map(|s| s.arch.as_str())
                    .unwrap_or("amd64");
                let results = self.online.search_contents(file_path, false, arch);
                Ok(results.into_iter().map(|r| r.pkg_name).collect())
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
                match local.find_by_name(name) {
                    Some(pkg) => Ok(pkg.requires.clone()),
                    None => Ok(Vec::new()),
                }
            }
            PackageSource::Repo => {
                let packages = self.get_repo_packages_ref(cfg, true)?;
                Ok(packages
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.requires.clone())
                    .unwrap_or_default())
            }
        }
    }

    fn get_reverse_dependencies(
        &self,
        name: &str,
        source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Vec<ReverseDep>> {
        match source {
            PackageSource::Installed => Ok(self.require_local()?.get_rdeps(name)),
            PackageSource::Repo => {
                let packages = self.get_repo_packages_ref(cfg, true)?;
                Ok(packages
                    .iter()
                    .filter_map(|p| {
                        let matches: Vec<String> = p
                            .requires
                            .iter()
                            .chain(&p.recommends)
                            .chain(&p.suggests)
                            .chain(&p.replaces)
                            .chain(&p.conflicts)
                            .chain(&p.obsoletes)
                            .filter(|d| d.name == name)
                            .map(|d| d.name.clone())
                            .collect();
                        if matches.is_empty() {
                            None
                        } else {
                            Some(ReverseDep {
                                pkg_name: p.name.clone(),
                                version: p.version.clone(),
                                arch: Some(p.arch.clone()),
                                source_repo: p.source_repo.clone(),
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

    fn search_by_file(&self, file_path: &str, _cfg: &CacheConfig) -> Result<Vec<SearchResult>> {
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

        let arch = self
            .sources
            .first()
            .map(|s| s.arch.as_str())
            .unwrap_or("amd64");
        let repo_results = self.online.search_contents(file_path, false, arch);
        for r in repo_results {
            if !results.iter().any(|e: &SearchResult| {
                e.pkg_name == r.pkg_name && e.matched_text == r.matched_text
            }) {
                results.push(r);
            }
        }

        Ok(results)
    }

    fn get_source_package(
        &self,
        name: &str,
        _source: PackageSource,
        cfg: &CacheConfig,
    ) -> Result<Option<SourcePackageInfo>> {
        let all_packages = self.get_repo_packages_ref(cfg, true)?;

        let source_name = if let Some(pkg) = all_packages.iter().find(|p| p.name == name) {
            pkg.source_pkg
                .as_ref()
                .map(|s| s.split_whitespace().next().unwrap_or(s))
                .unwrap_or(&pkg.name)
        } else if let Some(pkg) = self.require_local().ok().and_then(|l| l.find_by_name(name)) {
            pkg.source_pkg
                .as_ref()
                .map(|s| s.split_whitespace().next().unwrap_or(s))
                .unwrap_or(&pkg.name)
        } else {
            name
        };

        let binaries: Vec<BinaryPackageInfo> = all_packages
            .iter()
            .filter(|p| {
                let p_src = p
                    .source_pkg
                    .as_ref()
                    .map(|s| s.split_whitespace().next().unwrap_or(s))
                    .unwrap_or(&p.name);
                p_src == source_name
            })
            .map(|p| {
                let inst_ver = self.installed_version_arch(&p.name, &p.arch);
                BinaryPackageInfo {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    arch: p.arch.clone(),
                    installed: inst_ver.as_deref() == Some(&p.version),
                    source_repo: p.source_repo.clone(),
                }
            })
            .collect();

        if binaries.is_empty() {
            return Ok(None);
        }

        let representative = all_packages
            .iter()
            .find(|p| p.name == source_name)
            .or_else(|| {
                all_packages.iter().find(|p| {
                    p.source_pkg
                        .as_deref()
                        .map(|s| s.split_whitespace().next().unwrap_or(s))
                        == Some(source_name)
                })
            });

        // 头部版本优先取本地已安装版本（与 info 命令一致）
        let local_version = self
            .local
            .as_ref()
            .and_then(|l| l.find_by_name(source_name))
            .map(|p| p.version.clone());
        Ok(Some(SourcePackageInfo {
            name: source_name.to_string(),
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
            binaries: self.dedup_and_sort_binaries(binaries, source_name),
        }))
    }

    fn get_changelog(
        &self,
        name: &str,
        source: PackageSource,
        _cfg: &CacheConfig,
    ) -> Result<Vec<ChangelogEntry>> {
        match source {
            PackageSource::Installed => Ok(self.require_local()?.get_changelog(name)),
            PackageSource::Repo => Ok(Vec::new()),
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
            Some(regex::Regex::new(pattern).map_err(|e| {
                PkgError::InvalidArgument(format!(
                    "\u{65e0}\u{6548}\u{7684}\u{6b63}\u{5219}\u{8868}\u{8fbe}\u{5f0f}: {}",
                    e
                ))
            })?)
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

        let mut results = Vec::new();

        // 文件路径匹配语义：正则直接匹配；关键词走路径分段匹配（过滤噪声扩展名）
        let pattern_lower = pattern.to_lowercase();
        let file_match_fn = |path: &str| -> bool {
            if let Some(ref re) = re {
                return re.is_match(path);
            }
            if crate::backend::common::has_noise_extension(&path.to_lowercase()) {
                return false;
            }
            crate::backend::common::path_segment_match(path, &pattern_lower)
        };

        match source {
            PackageSource::Installed => {
                let local = self.require_local()?;
                for pkg in &local.packages {
                    if match_fn(&pkg.name) || match_fn(&pkg.summary) {
                        results.push(SearchResult {
                            pkg_name: pkg.name.clone(),
                            matched_text: pkg.summary.clone(),
                            match_type: "keyword".to_string(),
                            source: "installed".to_string(),
                        });
                    }
                }

                // 本地文件路径匹配：扫描 /var/lib/dpkg/info/*.list（支持 owns 通配符与关键词文件检索）
                let info_dir = std::path::PathBuf::from("/var/lib/dpkg/info");
                if let Ok(entries) = std::fs::read_dir(&info_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let is_list = path.extension().map(|e| e == "list").unwrap_or(false);
                        if !is_list {
                            continue;
                        }
                        let pkg_name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .map(|s| s.split(':').next().unwrap_or(&s).to_string())
                            .unwrap_or_default();
                        if pkg_name.is_empty() {
                            continue;
                        }
                        // 只读 mmap + 字节级按行扫描，避免整文件 String 分配。
                        let file = match std::fs::File::open(&path) {
                            Ok(f) => f,
                            Err(_) => continue,
                        };
                        // SAFETY: 只读映射 dpkg 的 .list 文件，本进程不写入；
                        // Mmap 以 RAII 管理映射生命周期，无悬垂指针。
                        let mmap = match unsafe { memmap2::Mmap::map(&file) } {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        for raw in mmap.split(|&b| b == b'\n') {
                            let line = match std::str::from_utf8(raw) {
                                Ok(s) => s.trim_end_matches('\r'),
                                Err(_) => continue,
                            };
                            if !line.starts_with('/') || !file_match_fn(line) {
                                continue;
                            }
                            results.push(SearchResult {
                                pkg_name: pkg_name.clone(),
                                matched_text: line.to_string(),
                                match_type: "file".to_string(),
                                source: "installed".to_string(),
                            });
                        }
                    }
                }
            }
            PackageSource::Repo => {
                let packages = self.get_repo_packages_ref(cfg, true)?;
                for p in packages {
                    if match_fn(&p.name) || match_fn(&p.summary) {
                        results.push(SearchResult {
                            pkg_name: p.name.clone(),
                            matched_text: p.summary.clone(),
                            match_type: "keyword".to_string(),
                            source: "repo".to_string(),
                        });
                    }
                }

                let arch = self
                    .sources
                    .first()
                    .map(|s| s.arch.as_str())
                    .unwrap_or("amd64");
                let repo_file_results = self.search_contents_cached(pattern, use_regex, arch, cfg);
                results.extend(repo_file_results);
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::common::{
        is_noise_ext_bytes as is_noise_ext, path_segment_match as segment_match,
    };

    #[test]
    fn test_segment_match() {
        assert!(segment_match("/usr/bin/unzip", "unzip"));
        assert!(segment_match("/usr/bin/unzip-bin", "unzip")); // 前缀命中 + '-' 边界
        assert!(!segment_match("/usr/bin/unzip2", "unzip")); // 数字边界不算
        assert!(segment_match("/usr/share/doc/vim", "vim"));
    }

    #[test]
    fn test_segment_match_full_path() {
        // P0-2：完整路径查询按整路径精确匹配
        assert!(segment_match("/usr/bin/lunzip", "/usr/bin/lunzip"));
        assert!(!segment_match("/usr/bin/unzip", "/usr/bin/lunzip"));
        assert!(segment_match("/usr/bin/zip/", "/usr/bin/zip")); // 尾斜杠归一
    }

    #[test]
    fn test_is_noise_ext() {
        assert!(is_noise_ext(b"/usr/share/icons/a.png"));
        assert!(is_noise_ext(b"/usr/share/x.PNG")); // 大小写不敏感
        assert!(!is_noise_ext(b"/usr/bin/pngview"));
    }

    #[test]
    fn test_line_aligned_chunks() {
        let data: Vec<u8> = b"line1\nline22\nline333\n".to_vec();
        let chunks = line_aligned_chunks(&data, 8);
        let joined: Vec<u8> = chunks.concat();
        assert_eq!(joined, data);
        for c in &chunks[..chunks.len() - 1] {
            assert_eq!(c[c.len() - 1], b'\n', "除最后一块外应按行边界切分");
        }
    }
}

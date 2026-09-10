use std::path::PathBuf;

mod header;
use header::*;

use crate::error::{PkgError, Result};
use crate::model::*;

const RPM_HEADER_MAGIC: [u8; 4] = [0x8e, 0xad, 0xe8, 0x01];

/// RPM 数据库路径探测：sqlite（rpm >= 4.16）/ NDB（Packages.db）/ BDB（Packages）
fn rpm_db_path() -> Option<PathBuf> {
    for p in [
        "/var/lib/rpm/rpmdb.sqlite",
        "/var/lib/rpm/Packages.db",
        "/var/lib/rpm/Packages",
    ] {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    None
}

#[derive(Default)]
pub struct RpmLocal {
    pub packages: Vec<PkgMetadata>,
    pub name_map: std::collections::HashMap<String, usize>,
    pub provides_map: std::collections::HashMap<String, String>,
    pub installed_nvr: std::collections::HashSet<(String, String, String, String)>,
}

impl RpmLocal {
    pub fn load() -> Result<Self> {
        let db_path = rpm_db_path().ok_or_else(|| {
            PkgError::DatabaseError(
                "No RPM database found (/var/lib/rpm/rpmdb.sqlite | Packages.db | Packages)".into(),
            )
        })?;
        Self::load_from(&db_path)
    }

    /// 从指定的 rpmdb 文件加载（测试或备用路径可注入）。
    pub fn load_from(db_path: &PathBuf) -> Result<Self> {
        // P1-1：解析结果缓存（以 rpmdb mtime 失效，不设 TTL）
        let cache_path = crate::cache::rpm_local_cache_path();
        if let Some(cached) =
            crate::cache::PkgIndexCache::load(&cache_path, 0, false, std::slice::from_ref(db_path))
        {
            return Ok(Self::from_packages(cached.packages));
        }

        let local = if db_path.extension().and_then(|e| e.to_str()) == Some("sqlite") {
            Self::load_from_sqlite(db_path)
        } else {
            Self::load_from_ndb(db_path)
        }?;
        let _ = crate::cache::PkgIndexCache::save(&cache_path, local.packages.clone());
        Ok(local)
    }

    /// 从已解析的包列表构造（缓存加载路径），重建派生索引
    fn from_packages(packages: Vec<PkgMetadata>) -> Self {
        let mut local = Self {
            packages,
            name_map: std::collections::HashMap::new(),
            provides_map: std::collections::HashMap::new(),
            installed_nvr: std::collections::HashSet::new(),
        };
        for (i, pkg) in local.packages.iter().enumerate() {
            local.name_map.insert(pkg.name.clone(), i);
        }
        local.build_provides_map();
        local.build_installed_nvr();
        local
    }

    /// 已安装包的精确 NVR.A 集合（name-version-release.arch）：
    /// 仓库包的安装状态必须精确到版本比对（同名不同版本 = 未安装）
    fn build_installed_nvr(&mut self) {
        for p in &self.packages {
            self.installed_nvr.insert((
                p.name.clone(),
                p.version.clone(),
                p.release.clone(),
                p.arch.clone(),
            ));
        }
    }

    /// 按包名判定安装状态（source 全景视图用：本地存在同名包即视为已安装，
    /// 版本差异属于“仓库有更新”而非“未安装”）
    pub fn is_installed_by_name(&self, name: &str) -> bool {
        self.name_map.contains_key(name)
    }

    /// 精确安装判定：name + version + release + arch 全部匹配
    pub fn is_installed_exact(&self, name: &str, version: &str, release: &str, arch: &str) -> bool {
        self.installed_nvr.contains(&(
            name.to_string(),
            version.to_string(),
            release.to_string(),
            arch.to_string(),
        ))
    }

    fn build_provides_map(&mut self) {
        for pkg in &self.packages {
            let pkg_name = &pkg.name;
            self.provides_map
                .entry(pkg_name.clone())
                .or_insert_with(|| pkg_name.clone());
            for prov in &pkg.provides {
                let cap = prov.split_whitespace().next().unwrap_or(prov);
                // 路径型 provides（如 /bin/sh）同样建立映射：
                // bash provides /bin/sh 是标准 RPM 符号链接依赖，deps 反查需要
                self.provides_map
                    .entry(cap.to_string())
                    .or_insert_with(|| pkg_name.clone());
            }
        }
    }

    fn load_from_sqlite(path: &PathBuf) -> Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        let mut stmt = conn.prepare("SELECT blob FROM Packages")?;
        let mut local = Self::default();

        let rows = stmt.query_map([], |row| {
            let blob: Vec<u8> = row.get(0)?;
            Ok(blob)
        })?;

        for blob_result in rows {
            let blob = blob_result?;
            if let Some(pkg) = parse_rpm_header(&blob) {
                if !pkg.name.is_empty() {
                    let idx = local.packages.len();
                    local.name_map.insert(pkg.name.clone(), idx);
                    local.packages.push(pkg);
                }
            }
        }

        Ok(local)
    }

    fn load_from_ndb(path: &PathBuf) -> Result<Self> {
        let data = std::fs::read(path)?;

        if data.len() >= 4 && &data[0..4] == b"RpmP" {
            Self::parse_ndb_format(&data)
        } else {
            Self::scan_bdb_format(&data)
        }
    }

    fn parse_ndb_format(data: &[u8]) -> Result<Self> {
        let mut local = Self::default();

        // 定位 NAME(1000)/STRING(6) 索引项模式（memmem 加速）
        let finder = memchr::memmem::Finder::new(&[
            0x00, 0x00, 0x03, 0xe8, // tag = 1000 (NAME)
            0x00, 0x00, 0x00, 0x06, // type = 6 (STRING)
        ]);

        let mut pos = 0;
        while pos < data.len() {
            let idx = match finder.find(&data[pos..]) {
                Some(i) => pos + i,
                None => break,
            };
            pos = idx + 8;

            if idx + 16 > data.len() {
                break;
            }

            let offset_field =
                u32::from_be_bytes([data[idx + 8], data[idx + 9], data[idx + 10], data[idx + 11]]);
            let count_field = u32::from_be_bytes([
                data[idx + 12],
                data[idx + 13],
                data[idx + 14],
                data[idx + 15],
            ]);

            if offset_field != 2 || count_field != 1 {
                continue;
            }

            // P1-3 修复：RPM header 索引按 tag 严格递增（格式保证），
            // 从 NAME 条目前向精确计数，替代原 BlbS 估算（估算偏小会截掉
            // 索引尾部的高位 tag：BASENAMES/DIRNAMES/DIRINDEXES 等，导致文件列表丢失）
            let nindex = count_ndb_index_entries(data, idx);
            if !(10..=500).contains(&nindex) {
                continue;
            }

            let data_start = idx + nindex * 16;
            if data_start + 4 > data.len() {
                continue;
            }

            let name_off = data_start + 2;
            let name_end = match data[name_off..].iter().position(|&b| b == 0) {
                Some(p) => name_off + p,
                None => continue,
            };

            let name = String::from_utf8_lossy(&data[name_off..name_end]).to_string();
            if name.is_empty()
                || !name.chars().all(|c| {
                    c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.' || c == '_'
                })
            {
                continue;
            }

            if let Some(pkg) = parse_ndb_header(data, idx, nindex, data_start) {
                if !pkg.name.is_empty() {
                    let pkg_idx = local.packages.len();
                    local.name_map.insert(pkg.name.clone(), pkg_idx);
                    local.packages.push(pkg);
                }
            }

            // 从数据区起点继续搜索（原实现误加两次 nindex*16，可能跳过后续包）
            pos = data_start;
        }

        if local.packages.is_empty() {
            Err(PkgError::DatabaseError(
                "Failed to parse any packages from ndb format".into(),
            ))
        } else {
            Ok(local)
        }
    }

    fn scan_bdb_format(data: &[u8]) -> Result<Self> {
        let mut local = Self::default();
        let mut pos = 0;

        while pos + 16 < data.len() {
            if data[pos..pos + 4] == RPM_HEADER_MAGIC {
                let reserved = u32::from_be_bytes([
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]);
                if reserved != 0 {
                    pos += 1;
                    continue;
                }

                let nindex = u32::from_be_bytes([
                    data[pos + 8],
                    data[pos + 9],
                    data[pos + 10],
                    data[pos + 11],
                ]) as usize;

                let hsize = u32::from_be_bytes([
                    data[pos + 12],
                    data[pos + 13],
                    data[pos + 14],
                    data[pos + 15],
                ]) as usize;

                let header_total = 16 + nindex * 16 + hsize;

                if pos + header_total > data.len() {
                    pos += 1;
                    continue;
                }

                let header_data = &data[pos..pos + header_total];
                if let Some(pkg) = parse_rpm_header(header_data) {
                    if !pkg.name.is_empty() {
                        let pkg_idx = local.packages.len();
                        local.name_map.insert(pkg.name.clone(), pkg_idx);
                        local.packages.push(pkg);
                    }
                }

                pos += header_total;
            } else {
                pos += 1;
            }
        }

        Ok(local)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&PkgMetadata> {
        self.name_map.get(name).and_then(|&i| self.packages.get(i))
    }

    pub fn get_package_files(&self, pkg_name: &str) -> Vec<String> {
        // 文件列表来自 rpmdb BASENAMES/DIRNAMES 解析，完整可靠（P1-3：不再回退 rpm 子进程）
        self.find_by_name(pkg_name)
            .map(|p| p.files.clone())
            .unwrap_or_default()
    }

    /// 文件归属查询（RPM 端语义与 DEB 对齐）：
    /// - 输入是文件 → 精确匹配拥有该文件的包（等价 rpm -qf）
    /// - 输入是目录 → 目录本身 + 目录内文件的拥有者聚合（去重）
    ///   （等价 `owns /usr/bin/*` 的聚合视角，与 dpkg -S 的目录语义一致）
    pub fn find_file_owner(&self, file_path: &str) -> Vec<String> {
        let target = file_path.trim_end_matches('/');
        let is_dir_query = std::path::Path::new(target).is_dir();
        let dir_prefix = format!("{}/", target);

        let mut owners = Vec::new();
        for p in &self.packages {
            let mut owned = false;
            for f in &p.files {
                let f = f.trim_end_matches('/');
                if f == target {
                    owned = true;
                    break;
                }
                // 目录查询：该包拥有此目录下的文件（含目录自身条目）
                if is_dir_query && (f.starts_with(&dir_prefix) || f == target) {
                    owned = true;
                    break;
                }
            }
            if owned {
                owners.push(p.name.clone());
            }
        }
        owners
    }

    pub fn search(&self, keyword: &str) -> Vec<&PkgMetadata> {
        let kw = keyword.to_lowercase();
        self.packages
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&kw)
                    || p.summary.to_lowercase().contains(&kw)
                    || p.description.to_lowercase().contains(&kw)
            })
            .collect()
    }

    pub fn get_rdeps(&self, name: &str) -> Vec<ReverseDep> {
        let target_caps = self.build_target_caps(name);
        self.packages
            .iter()
            .filter_map(|p| {
                if p.name == name {
                    return None;
                }
                let matches = check_pkg_requires(p, &target_caps);
                if matches.is_empty() {
                    None
                } else {
                    Some(ReverseDep {
                        pkg_name: p.name.clone(),
                        version: if !p.release.is_empty() {
                            format!("{}-{}", p.version, p.release)
                        } else {
                            p.version.clone()
                        },
                        arch: Some(p.arch.clone()),
                        source_repo: None,
                        matched_deps: matches,
                    })
                }
            })
            .collect()
    }

    /// Build the set of capabilities that identify a package:
    /// its name + all its provides (sonames, config(...), etc.) + main binary paths.
    pub fn build_target_caps(&self, name: &str) -> std::collections::HashSet<String> {
        let mut caps = std::collections::HashSet::new();
        caps.insert(name.to_string());
        if let Some(pkg) = self.find_by_name(name) {
            for prov in &pkg.provides {
                let cap = prov.split_whitespace().next().unwrap_or(prov);
                caps.insert(cap.to_string());
            }
            for f in &pkg.files {
                if f.starts_with("/usr/bin/")
                    || f.starts_with("/usr/sbin/")
                    || f.starts_with("/bin/")
                    || f.starts_with("/sbin/")
                {
                    caps.insert(f.clone());
                }
            }
        }
        caps
    }

    /// Resolve a dependency name (which may be a .so, config(...), file path, or bare name)
    /// to the real package name that provides it.
    pub fn resolve_dep_to_pkg(&self, dep_name: &str) -> Option<String> {
        let cap = dep_name.split_whitespace().next().unwrap_or(dep_name);
        // 1) 完整能力精确匹配（含版本符号，如 libc.so.6(GLIBC_2.38)(64bit)）
        if let Some(pkg_name) = self.provides_map.get(cap) {
            if self.name_map.contains_key(pkg_name) {
                return Some(pkg_name.clone());
            }
        }
        // 2) 裸名匹配
        if self.name_map.contains_key(cap) {
            return Some(cap.to_string());
        }
        // 3) soname 剥离版本符号后重试：libc.so.6(GLIBC_2.38)(64bit) →
        //    libc.so.6（glibc 的 provides 索引以裸 soname 建键）
        if cap.starts_with("lib") || cap.contains(".so") {
            if let Some(soname) = cap.split('(').next() {
                if soname != cap {
                    if let Some(pkg_name) = self.provides_map.get(soname) {
                        if self.name_map.contains_key(pkg_name) {
                            return Some(pkg_name.clone());
                        }
                    }
                    if self.name_map.contains_key(soname) {
                        return Some(soname.to_string());
                    }
                }
            }
        }
        // 4) 路径型 capability 由内存文件索引解析（P1-3，不再调用 rpm 子进程）
        if cap.starts_with('/') {
            return self.file_owner_in_memory(cap);
        }
        None
    }

    /// 内存文件归属查找（路径型依赖/能力解析）
    fn file_owner_in_memory(&self, path: &str) -> Option<String> {
        let target = path.trim_end_matches('/');
        self.packages
            .iter()
            .find(|p| p.files.iter().any(|f| f.trim_end_matches('/') == target))
            .map(|p| p.name.clone())
    }

    /// Resolve all requires of a package to real package names,
    /// filtering self-dependencies and deduplicating.
    pub fn resolve_deps_to_packages(&self, pkg_name: &str) -> Vec<Dependency> {
        let pkg = match self.find_by_name(pkg_name) {
            Some(p) => p,
            None => return Vec::new(),
        };
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut result = Vec::new();
        for req in &pkg.requires {
            let resolved = self
                .resolve_dep_to_pkg(&req.name)
                .unwrap_or_else(|| req.name.clone());
            if resolved == pkg_name {
                continue;
            }
            if resolved.contains('/') || !resolved.chars().any(|c| c.is_ascii_alphabetic()) {
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
        result
    }

    pub fn resolve_all_deps(&self, pkg_name: &str) -> DepGroups {
        let pkg = match self.find_by_name(pkg_name) {
            Some(p) => p,
            None => return Default::default(),
        };
        let resolve = |deps: &[Dependency]| -> Vec<Dependency> {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut result = Vec::new();
            for req in deps {
                let resolved = self
                    .resolve_dep_to_pkg(&req.name)
                    .unwrap_or_else(|| req.name.clone());
                if resolved == pkg_name {
                    continue;
                }
                if resolved.contains('/') || !resolved.chars().any(|c| c.is_ascii_alphabetic()) {
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
            result
        };
        (
            resolve(&pkg.requires),
            resolve(&pkg.recommends),
            resolve(&pkg.suggests),
            resolve(&pkg.conflicts),
            resolve(&pkg.obsoletes),
            resolve(&pkg.replaces),
        )
    }

    pub fn get_binaries_from_source(&self, source_name: &str) -> Vec<String> {
        self.packages
            .iter()
            .filter(|p| {
                if let Some(src) = &p.source_pkg {
                    let base = src.trim_end_matches(".src.rpm");
                    base == source_name || base.starts_with(&format!("{}-", source_name))
                } else {
                    false
                }
            })
            .map(|p| p.name.clone())
            .collect()
    }
}

/// Check if package P's **strong** requires (only Requires, not Recommends/Suggests)
/// match any of the target capabilities.
/// Returns the list of matched require names (empty if no match).
pub fn check_pkg_requires(
    p: &PkgMetadata,
    target_caps: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut matches = Vec::new();
    for req in &p.requires {
        let req_clean = req.name.split_whitespace().next().unwrap_or(&req.name);
        if target_caps.contains(req_clean) {
            matches.push(req.name.clone());
        }
    }
    matches
}

/// 六组依赖列表：requires/recommends/suggests/conflicts/obsoletes/replaces
type DepGroups = (
    Vec<Dependency>,
    Vec<Dependency>,
    Vec<Dependency>,
    Vec<Dependency>,
    Vec<Dependency>,
    Vec<Dependency>,
);

// Fuzz 入口（仅 `--features fuzzing` 编译，报告 H3）。
#[cfg(feature = "fuzzing")]
impl RpmLocal {
    /// NDB（Packages.db）启发式解析入口。
    pub fn fuzz_parse_ndb(data: &[u8]) {
        let _ = Self::parse_ndb_format(data);
    }

    /// BDB（Packages）魔数扫描解析入口。
    pub fn fuzz_scan_bdb(data: &[u8]) {
        let _ = Self::scan_bdb_format(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小合法的 RPM header blob（magic + index + data）
    fn make_header(entries: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut index = Vec::new();
        for (tag, type_id, payload) in entries {
            let offset = data.len() as u32;
            index.extend_from_slice(&tag.to_be_bytes());
            index.extend_from_slice(&type_id.to_be_bytes());
            index.extend_from_slice(&offset.to_be_bytes());
            index.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            data.extend_from_slice(payload);
        }
        let mut out = Vec::new();
        out.extend_from_slice(&RPM_HEADER_MAGIC);
        out.extend_from_slice(&0u32.to_be_bytes()); // reserved
        out.extend_from_slice(&(entries.len() as u32).to_be_bytes()); // nindex
        out.extend_from_slice(&(data.len() as u32).to_be_bytes()); // hsize
        out.extend_from_slice(&index);
        out.extend_from_slice(&data);
        out
    }

    fn cstr(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    #[test]
    fn test_parse_rpm_header_basic() {
        let blob = make_header(&[
            (TAG_NAME, TYPE_STRING, cstr("bash")),
            (TAG_VERSION, TYPE_STRING, cstr("5.2.15")),
            (TAG_RELEASE, TYPE_STRING, cstr("19.uos25")),
            (TAG_ARCH, TYPE_STRING, cstr("x86_64")),
            (TAG_SUMMARY, TYPE_STRING, cstr("GNU Bourne Again SHell")),
            (TAG_LICENSE, TYPE_STRING, cstr("GPLv3")),
            (
                TAG_SOURCERPM,
                TYPE_STRING,
                cstr("bash-5.2.15-19.uos25.src.rpm"),
            ),
        ]);
        let pkg = parse_rpm_header(&blob).expect("header 应可解析");
        assert_eq!(pkg.name, "bash");
        assert_eq!(pkg.version, "5.2.15");
        assert_eq!(pkg.release, "19.uos25");
        assert_eq!(pkg.arch, "x86_64");
        assert_eq!(pkg.summary, "GNU Bourne Again SHell");
        assert_eq!(pkg.license.as_deref(), Some("GPLv3"));
        assert_eq!(
            pkg.source_pkg.as_deref(),
            Some("bash-5.2.15-19.uos25.src.rpm")
        );
    }

    #[test]
    fn test_parse_rpm_header_rejects_invalid_version() {
        // version 无数字 → 拒绝（防误判启发式保护）
        let blob = make_header(&[
            (TAG_NAME, TYPE_STRING, cstr("ab")),
            (TAG_VERSION, TYPE_STRING, cstr("noversion")),
        ]);
        assert!(parse_rpm_header(&blob).is_none());
    }

    #[test]
    fn test_extract_files_from_header() {
        let mut dirs = cstr("/usr/bin/");
        dirs.extend(cstr("/etc/"));
        let mut basenames = cstr("bash");
        basenames.extend(cstr("bashrc"));
        let mut idx: Vec<u8> = Vec::new();
        idx.extend_from_slice(&0i32.to_be_bytes());
        idx.extend_from_slice(&1i32.to_be_bytes());
        let blob = make_header(&[
            (TAG_NAME, TYPE_STRING, cstr("testpkg")),
            (TAG_VERSION, TYPE_STRING, cstr("1.0")),
            (TAG_BASENAMES, TYPE_STRING_ARRAY, basenames),
            (TAG_DIRNAMES, TYPE_STRING_ARRAY, dirs),
            (TAG_DIRINDEXES, TYPE_INT32, idx),
        ]);
        let pkg = parse_rpm_header(&blob).expect("header 应可解析");
        assert_eq!(pkg.files, vec!["/usr/bin/bash", "/etc/bashrc"]);
    }

    #[test]
    fn test_flags_to_string() {
        assert_eq!(flags_to_string(0x0C), Some(">=".to_string())); // GREATER|EQUAL
        assert_eq!(flags_to_string(0x0A), Some("<=".to_string())); // LESS|EQUAL
        assert_eq!(flags_to_string(0x08), Some("=".to_string()));
        assert_eq!(flags_to_string(0x04), Some(">".to_string()));
        assert_eq!(flags_to_string(0x02), Some("<".to_string()));
        assert_eq!(flags_to_string(0), None);
        assert_eq!(flags_to_string(0x01), None); // 未知位组合
    }

    #[test]
    fn test_is_valid_pkg_name() {
        assert!(is_valid_pkg_name(b"bash"));
        assert!(is_valid_pkg_name(b"glibc-2.36_x.y+z"));
        assert!(!is_valid_pkg_name(b""));
        assert!(!is_valid_pkg_name(b"has space"));
        assert!(!is_valid_pkg_name(b"123")); // 无字母
    }

    /// NDB 实库回归（P1-3 修复验证）：需要 /tmp/opencode/Packages.db fixture。
    /// 防止回归：nindex 边界误判 / 文件三元组标签错位导致的文件列表丢失。
    /// fixture 采集：scp <rpm机>:/var/lib/rpm/Packages.db /tmp/opencode/
    #[test]
    fn regression_ndb_files_extraction() {
        let path = std::path::PathBuf::from("/tmp/opencode/Packages.db");
        if !path.exists() {
            return; // 无 fixture 环境跳过
        }
        let data = std::fs::read(&path).unwrap();
        let local = super::RpmLocal::parse_ndb_format(&data).expect("parse ndb");
        assert!(
            local.packages.len() > 1000,
            "应解析出全量包，实际 {}",
            local.packages.len()
        );
        let with_files = local
            .packages
            .iter()
            .filter(|p| !p.files.is_empty())
            .count();
        assert!(
            with_files > local.packages.len() * 9 / 10,
            "绝大多数包应有文件列表：{}/{}",
            with_files,
            local.packages.len()
        );
        let bash = local
            .packages
            .iter()
            .find(|p| p.name == "bash")
            .expect("bash");
        assert!(!bash.files.is_empty(), "bash 文件列表不应为空");
        assert!(bash
            .files
            .iter()
            .any(|f| f.contains("/bin/bash") || f.contains("bash")));
    }

    fn synthetic_pkg(name: &str, requires: &[&str]) -> PkgMetadata {
        PkgMetadata {
            name: name.into(),
            version: "1.0".into(),
            release: "1".into(),
            arch: "x86_64".into(),
            requires: requires
                .iter()
                .map(|r| Dependency {
                    name: (*r).into(),
                    version: None,
                    flags: None,
                    is_alternative: false,
                })
                .collect(),
            provides: vec![format!("{}.so.1", name)],
            ..Default::default()
        }
    }

    #[test]
    fn backend_rpm_from_packages_index_and_rdeps() {
        let local = RpmLocal::from_packages(vec![
            synthetic_pkg("glibc", &[]),
            synthetic_pkg("bash", &["glibc"]),
            synthetic_pkg("curl", &["bash", "glibc"]),
        ]);
        assert!(local.find_by_name("bash").is_some());
        assert!(local.is_installed_by_name("curl"));
        assert!(local.is_installed_exact("bash", "1.0", "1", "x86_64"));
        assert!(!local.is_installed_exact("bash", "2.0", "1", "x86_64"));
        // 能力名解析
        assert_eq!(local.resolve_dep_to_pkg("glibc"), Some("glibc".to_string()));
        // 反向依赖：依赖 bash 的包应包含 curl，且不含 bash 自身
        let rdeps = local.get_rdeps("bash");
        assert!(rdeps.iter().any(|r| r.pkg_name == "curl"));
        assert!(!rdeps.iter().any(|r| r.pkg_name == "bash"));
    }
}

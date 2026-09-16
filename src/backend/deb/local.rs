use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::*;

pub struct DebLocal {
    pub packages: Vec<PkgMetadata>,
    pub name_map: HashMap<String, usize>,
    pub name_arch_map: HashMap<(String, String), usize>,
}

impl DebLocal {
    pub fn load() -> Result<Self> {
        Self::load_from(Path::new("/var/lib/dpkg/status"))
    }

    /// 从指定的 dpkg status 文件加载（测试或备用路径可注入）。
    pub fn load_from(status_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(status_path)?;
        Ok(Self::from_control_text(&content))
    }

    /// 从 control 文本构建（纯函数，便于离线单测/端到端测试）。
    pub fn from_control_text(content: &str) -> Self {
        let packages = parse_control_paragraphs(content);

        let mut local = Self {
            packages,
            name_map: HashMap::new(),
            name_arch_map: HashMap::new(),
        };

        for (i, pkg) in local.packages.iter().enumerate() {
            local.name_map.insert(pkg.name.clone(), i);
            local
                .name_arch_map
                .insert((pkg.name.clone(), pkg.arch.clone()), i);
        }

        local
    }

    pub fn find_by_name(&self, name: &str) -> Option<&PkgMetadata> {
        self.name_map.get(name).and_then(|&i| self.packages.get(i))
    }

    pub fn find_by_name_arch(&self, name: &str, arch: &str) -> Option<&PkgMetadata> {
        self.name_arch_map
            .get(&(name.to_string(), arch.to_string()))
            .and_then(|&i| self.packages.get(i))
    }

    /// 惰性加载：仅在需要时单独读取指定包的文件列表
    pub fn get_package_files(&self, pkg_name: &str) -> Vec<String> {
        Self::read_package_files(&PathBuf::from("/var/lib/dpkg/info"), pkg_name)
    }

    /// 从指定的 dpkg info 目录读取（测试或备用路径可注入）。
    /// 多架构包（Multi-Arch: same）的 info 文件带架构限定符，
    /// 如 libgnutls-dane0:amd64.list；与 dpkg -L 语义对齐：
    /// 无未限定文件时合并该包全部架构的列表并去重。
    fn read_package_files(info_dir: &Path, pkg_name: &str) -> Vec<String> {
        if let Ok(content) = std::fs::read_to_string(info_dir.join(format!("{}.list", pkg_name))) {
            return content.lines().map(|l| l.to_string()).collect();
        }
        let prefix = format!("{}:", pkg_name);
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(info_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_list = path.extension().map(|e| e == "list").unwrap_or(false);
                let is_pkg = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false);
                if !(is_list && is_pkg) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    files.extend(content.lines().map(|l| l.to_string()));
                }
            }
        }
        files.sort();
        files.dedup();
        files
    }

    /// 仅在 owns 命令时流式查找拥有指定文件的包。
    /// 以只读 mmap + 字节级按行扫描替代逐文件 `read_to_string`，
    /// 避免为每个 `dpkg/info/*.list` 分配整段 `String`。
    pub fn find_file_owner(&self, file_path: &str) -> Vec<String> {
        let mut owners = Vec::new();
        let target = file_path.trim_end_matches('/').as_bytes();

        let info_dir = PathBuf::from("/var/lib/dpkg/info");
        if let Ok(entries) = std::fs::read_dir(info_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "list").unwrap_or(false) {
                    let matched = scan_lines(&path, |line| line.trim_ascii() == target);
                    if matched {
                        if let Some(file_stem) = path.file_stem() {
                            let pkg_name = file_stem.to_string_lossy().to_string();
                            // 处理例如 libc6:amd64.list 的情况
                            let clean_name =
                                pkg_name.split(':').next().unwrap_or(&pkg_name).to_string();
                            if !owners.contains(&clean_name) {
                                owners.push(clean_name);
                            }
                        }
                    }
                }
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
        let collect_matches = |p: &PkgMetadata| -> Vec<String> {
            p.requires
                .iter()
                .chain(&p.recommends)
                .chain(&p.suggests)
                .chain(&p.replaces)
                .chain(&p.conflicts)
                .chain(&p.obsoletes)
                .filter(|d| d.name == name)
                .map(|d| d.name.clone())
                .collect()
        };
        self.packages
            .iter()
            .filter_map(|p| {
                let matches = collect_matches(p);
                if matches.is_empty() {
                    None
                } else {
                    Some(ReverseDep {
                        pkg_name: p.name.clone(),
                        version: p.version.clone(),
                        arch: Some(p.arch.clone()),
                        source_repo: None,
                        matched_deps: matches,
                    })
                }
            })
            .collect()
    }

    pub fn get_changelog(&self, name: &str) -> Vec<ChangelogEntry> {
        let paths = [
            format!("/usr/share/doc/{}/changelog.Debian.gz", name),
            format!("/usr/share/doc/{}/changelog.gz", name),
        ];

        for path in &paths {
            if let Ok(entries) = read_changelog_gz(path) {
                if !entries.is_empty() {
                    return entries;
                }
            }
        }

        Vec::new()
    }

    pub fn get_binaries_from_source(&self, source_name: &str) -> Vec<String> {
        self.packages
            .iter()
            .filter(|p| p.source_pkg.as_deref() == Some(source_name))
            .map(|p| p.name.clone())
            .collect()
    }
}

/// 只读映射 `path` 并按行调用 `f`，`f` 返回 `true` 时提前结束。
/// 文件不可读或映射失败时返回 `false`（等价于「未命中」）。
fn scan_lines<F: FnMut(&[u8]) -> bool>(path: &Path, f: F) -> bool {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    // SAFETY: 只读映射 dpkg 的 .list 文件，本进程不写入；Mmap 以 RAII 管理
    // 映射生命周期，无悬垂指针；映射失败时按未命中处理。
    let mmap = match unsafe { memmap2::Mmap::map(&file) } {
        Ok(mmap) => mmap,
        Err(_) => return false,
    };
    mmap.split(|&b| b == b'\n').any(f)
}

fn read_changelog_gz(path: &str) -> Result<Vec<ChangelogEntry>> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut content = String::new();
    decoder.read_to_string(&mut content)?;

    Ok(parse_changelog(&content))
}

fn parse_changelog(content: &str) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let mut current_text = String::new();

    for line in content.lines() {
        if line.trim_start().starts_with("-- ") {
            let rest = &line.trim_start()[3..];
            let mut parts = rest.rsplit("  ");
            let _date = parts.next().unwrap_or("").trim();
            let author = parts
                .next()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| rest.trim().to_string());

            entries.push(ChangelogEntry {
                author,
                timestamp: 0,
                text: current_text.trim().to_string(),
            });
            current_text = String::new();
        } else if !line.is_empty() && !line.starts_with("  ") {
            current_text = String::new();
        } else if line.starts_with("  ") {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if !current_text.is_empty() {
                    current_text.push('\n');
                }
                current_text.push_str(trimmed);
            }
        }
    }

    entries
}

pub fn parse_control_paragraphs(content: &str) -> Vec<PkgMetadata> {
    let mut packages = Vec::new();
    let paragraphs: Vec<&str> = content.split("\n\n").collect();

    for para in paragraphs {
        if para.trim().is_empty() {
            continue;
        }

        let fields = parse_control_fields(para);
        let status = fields.get("Status").map(|s| s.as_str()).unwrap_or("");

        if status.is_empty() {
            // No Status field
        } else if !status.split_whitespace().any(|w| w == "installed") {
            continue;
        }

        if let Some(name) = fields.get("Package") {
            let deps = parse_depends(fields.get("Depends"));
            let pre_deps = parse_depends(fields.get("Pre-Depends"));
            let mut requires = deps;
            requires.extend(pre_deps);

            let recommends = parse_depends(fields.get("Recommends"));
            let suggests = parse_depends(fields.get("Suggests"));
            let conflicts = parse_depends(fields.get("Conflicts"));
            let obsoletes = parse_depends(fields.get("Breaks"));
            let replaces = parse_depends(fields.get("Replaces"));

            let provides: Vec<String> = fields
                .get("Provides")
                .map(|s| {
                    s.split(',')
                        .map(|d| d.split_whitespace().next().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();

            let description = fields.get("Description").cloned().unwrap_or_default();
            let (summary, desc) = if let Some(pos) = description.find('\n') {
                (
                    description[..pos].to_string(),
                    clean_description(&description[pos + 1..]),
                )
            } else {
                (description.clone(), String::new())
            };

            let source_pkg = fields.get("Source").cloned();

            let pkg = PkgMetadata {
                name: name.clone(),
                version: fields.get("Version").cloned().unwrap_or_default(),
                release: String::new(),
                epoch: None,
                arch: fields.get("Architecture").cloned().unwrap_or_default(),
                summary,
                description: desc,
                url: fields.get("Homepage").cloned(),
                license: None,
                vendor: None,
                packager: fields.get("Maintainer").cloned(),
                source_pkg,
                size: None,
                install_size: fields
                    .get("Installed-Size")
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|v| v * 1024),
                group: fields.get("Section").cloned(),
                priority: fields.get("Priority").cloned(),
                build_time: None,
                location: fields.get("Filename").cloned(),
                source_repo: None,
                requires,
                recommends,
                suggests,
                provides,
                conflicts,
                obsoletes,
                replaces,
                files: Vec::new(),
                changelog: Vec::new(),
            };

            packages.push(pkg);
        }
    }

    packages
}

fn parse_control_fields(para: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut current_key: Option<String> = None;
    let mut current_val = String::new();

    for line in para.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_key.is_some() {
                current_val.push('\n');
                current_val.push_str(line.trim_end());
            }
        } else if let Some(pos) = line.find(':') {
            if let Some(key) = current_key.take() {
                fields.insert(key, current_val.trim().to_string());
            }
            current_key = Some(line[..pos].trim().to_string());
            current_val = line[pos + 1..].trim().to_string();
        }
    }

    if let Some(key) = current_key {
        fields.insert(key, current_val.trim().to_string());
    }

    fields
}

fn parse_depends(field: Option<&String>) -> Vec<Dependency> {
    match field {
        None => Vec::new(),
        Some(s) => s
            .split(',')
            .flat_map(|dep| {
                let alternatives: Vec<&str> = dep.split('|').collect();
                let alts_len = alternatives.len();
                alternatives
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, alt)| {
                        let alt = alt.trim();
                        if alt.is_empty() {
                            return None;
                        }

                        let (name_part, version, flags) = if let Some(pos) = alt.find('(') {
                            let name = alt[..pos].trim();
                            let ver_part = alt[pos..].trim();
                            let inner = ver_part.trim_start_matches('(').trim_end_matches(')');
                            let mut iparts = inner.split_whitespace();
                            let flag = iparts.next().unwrap_or("");
                            let ver = iparts.next().unwrap_or("");
                            (name, Some(ver.to_string()), Some(flag.to_string()))
                        } else {
                            (alt, None, None)
                        };

                        let name = name_part
                            .trim_end_matches(":any")
                            .trim_end_matches(":native");

                        if name.is_empty() {
                            return None;
                        }

                        Some(Dependency {
                            name: name.to_string(),
                            version,
                            flags,
                            is_alternative: i + 1 < alts_len,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect(),
    }
}

fn clean_description(text: &str) -> String {
    let mut result = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed == "." {
            result.push(String::new());
        } else {
            result.push(trimmed.to_string());
        }
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_control_status_filter() {
        let content = "Package: aaa\nStatus: install ok installed\nVersion: 1.0\n\n\
                       Package: bbb\nStatus: deinstall ok config-files\nVersion: 2.0\n\n\
                       Package: ccc\nVersion: 3.0\n";
        let pkgs = parse_control_paragraphs(content);
        // bbb 为 config-files 应被过滤；ccc 无 Status 字段按现状保留
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "aaa");
        assert_eq!(pkgs[1].name, "ccc");
    }

    #[test]
    fn test_parse_control_fields_full() {
        let content = "Package: nginx\n\
                       Version: 1.20.1\n\
                       Architecture: amd64\n\
                       Source: nginx-src\n\
                       Homepage: http://nginx.org\n\
                       Maintainer: UOS <a@b.c>\n\
                       Section: web\n\
                       Priority: optional\n\
                       Installed-Size: 2000\n\
                       Status: install ok installed\n\
                       Depends: libc6 (>= 2.14), zlib1g (>= 1:1.2.0)\n\
                       Recommends: geoip-db\n\
                       Conflicts: nginx-common (<< 1.0)\n\
                       Breaks: old-thing\n\
                       Replaces: old-thing\n\
                       Provides: httpd, webserver\n\
                       Description: Web server\n\x20Some long description\n\x20more text\n";
        let pkgs = parse_control_paragraphs(content);
        assert_eq!(pkgs.len(), 1);
        let p = &pkgs[0];
        assert_eq!(p.name, "nginx");
        assert_eq!(p.version, "1.20.1");
        assert_eq!(p.arch, "amd64");
        assert_eq!(p.source_pkg.as_deref(), Some("nginx-src"));
        assert_eq!(p.url.as_deref(), Some("http://nginx.org"));
        assert_eq!(p.summary, "Web server");
        assert!(p.description.contains("long description"));
        assert_eq!(p.install_size, Some(2000 * 1024));
        assert_eq!(p.requires.len(), 2);
        assert_eq!(p.requires[0].name, "libc6");
        assert_eq!(p.requires[0].flags.as_deref(), Some(">="));
        assert_eq!(p.requires[0].version.as_deref(), Some("2.14"));
        assert_eq!(p.requires[1].name, "zlib1g");
        assert_eq!(p.requires[1].version.as_deref(), Some("1:1.2.0"));
        assert_eq!(p.recommends.len(), 1);
        assert_eq!(p.conflicts[0].flags.as_deref(), Some("<<"));
        assert_eq!(p.obsoletes[0].name, "old-thing"); // Breaks → obsoletes
        assert_eq!(p.replaces[0].name, "old-thing");
        assert!(p.provides.contains(&"httpd".to_string()));
        assert!(p.provides.contains(&"webserver".to_string()));
        assert_eq!(p.group.as_deref(), Some("web"));
        assert_eq!(p.priority.as_deref(), Some("optional"));
    }

    #[test]
    fn test_parse_depends_alternatives() {
        let field = "zlib1g | libz-dev, libc6:any (>= 2.0)".to_string();
        let deps = parse_depends(Some(&field));
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].name, "zlib1g");
        assert!(deps[0].is_alternative);
        assert_eq!(deps[1].name, "libz-dev");
        assert!(!deps[1].is_alternative);
        assert_eq!(deps[2].name, "libc6"); // :any 限定符被剥离
        assert_eq!(deps[2].version.as_deref(), Some("2.0"));
    }

    #[test]
    fn test_parse_changelog_entries() {
        let content = "bash (5.0-1) unstable; urgency=medium\n\n\
                       \x20 * fix bug one\n\
                       \x20 * fix bug two\n\n\
                       \x20 -- Author Name <a@b.c>  Mon, 01 Jan 2024 00:00:00 +0000\n";
        let entries = parse_changelog(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].author, "Author Name <a@b.c>");
        // 按现状：text 保留 "* " 前缀（trim 仅去空白）
        assert_eq!(entries[0].text, "* fix bug one\n* fix bug two");
        assert_eq!(entries[0].timestamp, 0); // DEB changelog 时间戳当前不解析
    }

    #[test]
    fn test_get_package_files_multi_arch() {
        let dir = std::env::temp_dir().join(format!("pkq_info_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("libc6:amd64.list"), "/a\n/b\n").unwrap();
        std::fs::write(dir.join("libc6:i386.list"), "/b\n/c\n").unwrap();
        // 非 .list 文件与不同包名前缀不应纳入
        std::fs::write(dir.join("libc6:amd64.md5sums"), "junk").unwrap();
        std::fs::write(dir.join("libc.list"), "/d\n").unwrap();
        let files = DebLocal::read_package_files(&dir, "libc6");
        assert_eq!(files, vec!["/a", "/b", "/c"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    const STATUS_FIXTURE: &str = "\
Package: bash
Status: install ok installed
Version: 5.0.4-deepin1
Architecture: amd64
Maintainer: Maintainer <m@example.com>
Installed-Size: 1000
Section: shells
Priority: required
Source: bash
Depends: libc6 (>= 2.28), libtinfo6
Recommends: bash-completion
Description: GNU Bourne Again SHell
 shell.

Package: curl
Status: install ok installed
Version: 7.74.0
Architecture: amd64
Depends: bash, libcurl4
Description: command line tool
 curl.

Package: not-installed
Status: deinstall ok config-files
Version: 1.0
Architecture: amd64
Description: removed package
";

    #[test]
    fn backend_deb_from_control_text_end_to_end() {
        let local = DebLocal::from_control_text(STATUS_FIXTURE);
        // 仅 Status 含 installed 的段落进入结果
        assert!(local.find_by_name("bash").is_some());
        assert!(local.find_by_name("curl").is_some());
        assert!(local.find_by_name("not-installed").is_none());
        // 架构索引
        assert!(local.find_by_name_arch("bash", "amd64").is_some());
        // name/summary/description 检索
        let hits = local.search("shell");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "bash");
        // 反向依赖：curl 依赖 bash
        let rdeps = local.get_rdeps("bash");
        assert!(rdeps.iter().any(|r| r.pkg_name == "curl"));
        // 源码包反查
        let bins = local.get_binaries_from_source("bash");
        assert!(bins.contains(&"bash".to_string()));
    }
}

use std::path::PathBuf;

use crate::error::{PkgError, Result};
use crate::model::*;
use crate::network::{FetchRequest, NetworkCacheManager, RepoQuery};

pub struct RpmOnline {
    cache: NetworkCacheManager,
}

/// repomd.xml 中 primary 数据项的关键信息
pub struct RepomdMeta {
    pub primary_href: String,
    pub primary_ts: i64,
}

impl Default for RpmOnline {
    fn default() -> Self {
        Self::new()
    }
}

impl RpmOnline {
    pub fn new() -> Self {
        Self {
            cache: NetworkCacheManager::new(),
        }
    }

    /// 拉取（ETag/TTL）并解析 repomd.xml，再拉取 primary（组合便捷方法）。
    pub fn fetch_and_parse_primary(&self, q: RepoQuery<'_>) -> Result<Vec<PkgMetadata>> {
        let meta = self.fetch_repomd_meta(q)?;
        self.fetch_primary_packages(q, &meta.primary_href)
    }

    /// 拉取并解析 repomd.xml，返回指定数据项的 `(href, timestamp)`。
    /// 解析失败视为缓存损坏：删除并强制重拉一次自愈；仍失败返回 `None`。
    pub fn fetch_repomd_entry(
        &self,
        q: RepoQuery<'_>,
        data_type: &str,
    ) -> Result<Option<(String, i64)>> {
        let repomd_url = format!("{}/repodata/repomd.xml", q.base_url);
        let repomd_path = self.cache.fetch_index(q.request(&repomd_url))?;
        let repomd_content = std::fs::read_to_string(&repomd_path)?;
        if let Some(entry) = parse_repomd_data(&repomd_content, data_type) {
            return Ok(Some(entry));
        }
        // 缓存文件损坏（如网络截断写入的空/残缺 repomd）：删除损坏文件
        // 并强制在线重拉一次自愈
        let _ = std::fs::remove_file(&repomd_path);
        let retry = FetchRequest {
            force: true,
            ..q.request(&repomd_url)
        };
        let retry_path = self.cache.fetch_index(retry)?;
        let retry_content = std::fs::read_to_string(&retry_path)?;
        Ok(parse_repomd_data(&retry_content, data_type))
    }

    /// 拉取（ETag/TTL）并解析 repomd.xml，返回 primary 数据项位置与时间戳（P1-1 缓存失效键）
    pub fn fetch_repomd_meta(&self, q: RepoQuery<'_>) -> Result<RepomdMeta> {
        match self.fetch_repomd_entry(q, "primary")? {
            Some((primary_href, primary_ts)) => Ok(RepomdMeta {
                primary_href,
                primary_ts,
            }),
            None => Err(PkgError::ParseError(
                "primary data not found in repomd.xml".into(),
            )),
        }
    }

    /// 按 repomd 给出的位置拉取 primary 并流式解压解析（P1-2）
    pub fn fetch_primary_packages(
        &self,
        q: RepoQuery<'_>,
        primary_href: &str,
    ) -> Result<Vec<PkgMetadata>> {
        let primary_url = format!("{}/{}", q.base_url, primary_href);
        let primary_path = self.cache.fetch_index(q.request(&primary_url))?;

        let file = std::fs::File::open(&primary_path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let buffered = std::io::BufReader::with_capacity(64 * 1024, gz);
        let mut packages = parse_primary_xml_stream(buffered)?;
        // 标注来源仓库（rdeps/来源标签依赖此字段区分已安装/仓库包）
        for p in &mut packages {
            p.source_repo = Some(q.repo_id.to_string());
        }
        Ok(packages)
    }

    pub fn fetch_and_parse_filelists(
        &self,
        q: RepoQuery<'_>,
    ) -> Result<(HashMap<String, Vec<String>>, i64)> {
        let (filelists_href, ts) = match self.fetch_repomd_entry(q, "filelists")? {
            Some(entry) => entry,
            None => return Ok((HashMap::new(), 0)),
        };

        let filelists_url = format!("{}/{}", q.base_url, filelists_href);
        let filelists_path = self.cache.fetch_index(q.request(&filelists_url))?;

        // 流式解压解析（避免全量解压入内存，P0-1/P1-2）
        let file = std::fs::File::open(&filelists_path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let buffered = std::io::BufReader::with_capacity(64 * 1024, gz);
        let map = parse_filelists_xml_stream(buffered);
        Ok((map, ts))
    }

    pub fn fetch_and_parse_other(
        &self,
        q: RepoQuery<'_>,
    ) -> Result<HashMap<String, Vec<ChangelogEntry>>> {
        let repomd_url = format!("{}/repodata/repomd.xml", q.base_url);
        let repomd_path = self.cache.fetch_index(q.request(&repomd_url))?;
        let repomd_content = std::fs::read_to_string(&repomd_path)?;

        let other_href = match parse_repomd_for_type(&repomd_content, "other") {
            Some(h) => h,
            None => return Ok(HashMap::new()),
        };

        let other_url = format!("{}/{}", q.base_url, other_href);
        let other_path = self.cache.fetch_index(q.request(&other_url))?;

        let other_xml = decompress_gz(&other_path)?;
        let map = parse_other_xml(&other_xml);
        Ok(map)
    }
}

use std::collections::HashMap;

fn decompress_gz(path: &PathBuf) -> Result<String> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut content = String::new();
    decoder.read_to_string(&mut content)?;
    Ok(content)
}

/// 解析 repomd.xml，返回指定数据项的 (location href, timestamp)
fn parse_repomd_data(repomd_xml: &str, data_type: &str) -> Option<(String, i64)> {
    let mut reader = quick_xml::Reader::from_str(repomd_xml);
    let mut buf = Vec::new();
    let mut in_target_data = false;
    let mut current_type = String::new();
    let mut location: Option<String> = None;
    let mut timestamp: i64 = 0;
    let mut in_timestamp = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                if e.name().as_ref() == b"data" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"type" {
                            current_type = String::from_utf8_lossy(attr.value.as_ref()).to_string();
                        }
                    }
                    in_target_data = current_type == data_type;
                } else if in_target_data && e.name().as_ref() == b"location" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"href" {
                            location =
                                Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
                        }
                    }
                } else if in_target_data && e.name().as_ref() == b"timestamp" {
                    in_timestamp = true;
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                if in_target_data && e.name().as_ref() == b"location" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"href" {
                            location =
                                Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                if in_timestamp {
                    timestamp = t
                        .decode()
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0);
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                if e.name().as_ref() == b"timestamp" {
                    in_timestamp = false;
                } else if e.name().as_ref() == b"data" {
                    if in_target_data {
                        if let Some(loc) = location {
                            return Some((loc, timestamp));
                        }
                        return None;
                    }
                    in_target_data = false;
                    location = None;
                    timestamp = 0;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    None
}

/// 兼容接口：仅取 location href
fn parse_repomd_for_type(repomd_xml: &str, data_type: &str) -> Option<String> {
    parse_repomd_data(repomd_xml, data_type).map(|(href, _)| href)
}

#[cfg(test)]
fn parse_primary_xml(xml: &str) -> Result<Vec<PkgMetadata>> {
    parse_primary_xml_stream(std::io::Cursor::new(xml))
}

fn parse_primary_xml_stream<R: std::io::BufRead>(reader: R) -> Result<Vec<PkgMetadata>> {
    let mut reader = quick_xml::Reader::from_reader(reader);
    let mut buf = Vec::new();
    let mut packages = Vec::new();
    let mut current_pkg: Option<PkgMetadata> = None;
    let mut current_field = String::new();
    let mut in_format = false;
    let mut in_rpm_section: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                let ln = name.local_name();
                let local = String::from_utf8_lossy(ln.as_ref());
                match local.as_ref() {
                    "package" => {
                        current_pkg = Some(PkgMetadata::default());
                    }
                    "format" => {
                        in_format = true;
                    }
                    "entry" if in_format => {
                        if let Some(pkg) = &mut current_pkg {
                            let entry_name = e
                                .attributes()
                                .find(|a| {
                                    a.as_ref()
                                        .map(|a| a.key.as_ref() == b"name")
                                        .unwrap_or(false)
                                })
                                .and_then(|a| a.ok())
                                .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string());
                            if let Some(en) = entry_name {
                                match in_rpm_section.as_deref() {
                                    Some("provides") => pkg.provides.push(en),
                                    Some("requires") => {
                                        let dep = parse_rpm_entry(&e);
                                        // 与本地解析路径一致：过滤 rpmlib/rtld 内部能力
                                        if !dep.name.starts_with("rpmlib(")
                                            && !dep.name.starts_with("rtld(")
                                        {
                                            pkg.requires.push(dep);
                                        }
                                    }
                                    Some("conflicts") => {
                                        let dep = parse_rpm_entry(&e);
                                        pkg.conflicts.push(dep);
                                    }
                                    Some("obsoletes") => {
                                        let dep = parse_rpm_entry(&e);
                                        pkg.obsoletes.push(dep);
                                    }
                                    Some("recommends") => {
                                        let dep = parse_rpm_entry(&e);
                                        pkg.recommends.push(dep);
                                    }
                                    Some("suggests") => {
                                        let dep = parse_rpm_entry(&e);
                                        pkg.suggests.push(dep);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {
                        if in_format
                            && (local == "provides"
                                || local == "requires"
                                || local == "conflicts"
                                || local == "obsoletes"
                                || local == "recommends"
                                || local == "suggests")
                        {
                            in_rpm_section = Some(local.to_string());
                        }
                        current_field = local.to_string();
                    }
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                let name = e.name();
                let ln = name.local_name();
                let local = String::from_utf8_lossy(ln.as_ref());
                if in_format && local == "entry" {
                    if let Some(pkg) = &mut current_pkg {
                        let entry_name = e
                            .attributes()
                            .find(|a| {
                                a.as_ref()
                                    .map(|a| a.key.as_ref() == b"name")
                                    .unwrap_or(false)
                            })
                            .and_then(|a| a.ok())
                            .map(|a| String::from_utf8_lossy(a.value.as_ref()).to_string());
                        if let Some(en) = entry_name {
                            match in_rpm_section.as_deref() {
                                Some("provides") => pkg.provides.push(en),
                                Some("requires") => {
                                    let dep = parse_rpm_entry(&e);
                                    // 与本地解析路径一致：过滤 rpmlib/rtld 内部能力
                                    if !dep.name.starts_with("rpmlib(")
                                        && !dep.name.starts_with("rtld(")
                                    {
                                        pkg.requires.push(dep);
                                    }
                                }
                                Some("conflicts") => {
                                    let dep = parse_rpm_entry(&e);
                                    pkg.conflicts.push(dep);
                                }
                                Some("obsoletes") => {
                                    let dep = parse_rpm_entry(&e);
                                    pkg.obsoletes.push(dep);
                                }
                                Some("recommends") => {
                                    let dep = parse_rpm_entry(&e);
                                    pkg.recommends.push(dep);
                                }
                                Some("suggests") => {
                                    let dep = parse_rpm_entry(&e);
                                    pkg.suggests.push(dep);
                                }
                                _ => {}
                            }
                        }
                    }
                } else if let Some(pkg) = &mut current_pkg {
                    let local_name = name.local_name();
                    let local = String::from_utf8_lossy(local_name.as_ref());
                    match local.as_ref() {
                        "version" => {
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"epoch" => {
                                        // epoch=0 等价于无 epoch（dnf 同语义不显示）
                                        let v = String::from_utf8_lossy(attr.value.as_ref())
                                            .to_string();
                                        if v != "0" {
                                            pkg.epoch = Some(v);
                                        }
                                    }
                                    b"ver" => {
                                        pkg.version =
                                            String::from_utf8_lossy(attr.value.as_ref()).to_string()
                                    }
                                    b"rel" => {
                                        pkg.release =
                                            String::from_utf8_lossy(attr.value.as_ref()).to_string()
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "time" => {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"build" {
                                    let s =
                                        String::from_utf8_lossy(attr.value.as_ref()).to_string();
                                    pkg.build_time = s.parse().ok();
                                }
                            }
                        }
                        "size" => {
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"package" => {
                                        let s = String::from_utf8_lossy(attr.value.as_ref())
                                            .to_string();
                                        pkg.size = s.parse().ok();
                                    }
                                    b"installed" => {
                                        let s = String::from_utf8_lossy(attr.value.as_ref())
                                            .to_string();
                                        pkg.install_size = s.parse().ok();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "location" => {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"href" {
                                    pkg.location = Some(
                                        String::from_utf8_lossy(attr.value.as_ref()).to_string(),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Some(pkg) = &mut current_pkg {
                    let text = t.decode().unwrap_or_default().to_string();
                    match current_field.as_str() {
                        "name" => pkg.name = text,
                        "arch" => pkg.arch = text,
                        "summary" => pkg.summary = text,
                        "description" => pkg.description = text,
                        "url" => pkg.url = Some(text),
                        "license" => pkg.license = Some(text),
                        "vendor" => pkg.vendor = Some(text),
                        "group" => pkg.group = Some(text),
                        "priority" => pkg.priority = Some(text),
                        "packager" => pkg.packager = Some(text),
                        "sourcerpm" => pkg.source_pkg = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e.name();
                let ln = name.local_name();
                let local = String::from_utf8_lossy(ln.as_ref());
                match local.as_ref() {
                    "package" => {
                        if let Some(pkg) = current_pkg.take() {
                            if !pkg.name.is_empty() {
                                packages.push(pkg);
                            }
                        }
                    }
                    "format" => {
                        in_format = false;
                    }
                    "provides" | "requires" | "conflicts" | "obsoletes" | "recommends"
                    | "suggests" => {
                        in_rpm_section = None;
                    }
                    _ => {}
                }
                current_field.clear();
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(PkgError::XmlError(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(packages)
}

/// Fuzz 入口（仅 `--features fuzzing` 编译，报告 H3）：primary.xml 流式解析。
#[cfg(feature = "fuzzing")]
pub fn fuzz_primary_xml(data: &[u8]) {
    let _ = parse_primary_xml_stream(std::io::Cursor::new(data));
}

fn parse_rpm_entry(e: &quick_xml::events::BytesStart) -> Dependency {
    let mut name = String::new();
    let mut ver = None;
    let mut flags = None;

    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"name" => name = String::from_utf8_lossy(attr.value.as_ref()).to_string(),
            b"ver" => ver = Some(String::from_utf8_lossy(attr.value.as_ref()).to_string()),
            b"flags" => flags = Some(String::from_utf8_lossy(attr.value.as_ref()).to_string()),
            _ => {}
        }
    }

    Dependency {
        name,
        version: ver,
        flags,
        is_alternative: false,
    }
}

fn parse_filelists_xml_stream<R: std::io::BufRead>(reader: R) -> HashMap<String, Vec<String>> {
    let mut reader = quick_xml::Reader::from_reader(reader);
    let mut buf = Vec::new();
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_pkg_name: Option<String> = None;
    let mut current_files: Vec<String> = Vec::new();
    let mut skip_file = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"package" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"name" {
                                current_pkg_name =
                                    Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
                                current_files.clear();
                            }
                        }
                    }
                    b"file" => {
                        skip_file = false;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" {
                                let val = String::from_utf8_lossy(attr.value.as_ref());
                                if val == "dir" || val == "ghost" {
                                    skip_file = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                if e.name().as_ref() == b"file" {
                    // self-closing file tag - unlikely but handle
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                let path = t.decode().unwrap_or_default().to_string();
                if !path.trim().is_empty() && current_pkg_name.is_some() && !skip_file {
                    // 归一为绝对路径（filelists 条目通常以 / 开头，防御性补齐）
                    if path.starts_with('/') {
                        current_files.push(path);
                    } else {
                        current_files.push(format!("/{}", path));
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                if e.name().as_ref() == b"package" {
                    if let Some(name) = current_pkg_name.take() {
                        map.insert(name, std::mem::take(&mut current_files));
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    map
}

fn parse_other_xml(xml: &str) -> HashMap<String, Vec<ChangelogEntry>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut map: HashMap<String, Vec<ChangelogEntry>> = HashMap::new();
    let mut current_pkg_name: Option<String> = None;
    let mut current_changelog: Vec<ChangelogEntry> = Vec::new();
    let mut current_entry: Option<ChangelogEntry> = None;
    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"package" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"name" {
                                current_pkg_name =
                                    Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
                                current_changelog.clear();
                            }
                        }
                    }
                    b"changelog" => {
                        let mut author = String::new();
                        let mut timestamp: i64 = 0;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"author" => {
                                    author =
                                        String::from_utf8_lossy(attr.value.as_ref()).to_string()
                                }
                                b"date" => {
                                    let s =
                                        String::from_utf8_lossy(attr.value.as_ref()).to_string();
                                    timestamp = s.parse().unwrap_or(0);
                                }
                                _ => {}
                            }
                        }
                        current_entry = Some(ChangelogEntry {
                            author,
                            timestamp,
                            text: String::new(),
                        });
                        current_tag = "changelog".to_string();
                    }
                    _ => {
                        current_tag =
                            String::from_utf8_lossy(name.local_name().as_ref()).to_string();
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                if current_tag == "changelog" {
                    if let Some(entry) = &mut current_entry {
                        entry.text = t.decode().unwrap_or_default().to_string();
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                match e.name().as_ref() {
                    b"package" => {
                        if let Some(name) = current_pkg_name.take() {
                            map.insert(name, std::mem::take(&mut current_changelog));
                        }
                    }
                    b"changelog" => {
                        if let Some(entry) = current_entry.take() {
                            current_changelog.push(entry);
                        }
                    }
                    _ => {}
                }
                current_tag.clear();
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm">
<package type="rpm">
  <name>nginx</name>
  <arch>x86_64</arch>
  <version epoch="0" ver="1.20.1" rel="1.uos25"/>
  <time file="1690000001" build="1690000000"/>
  <size package="1024000" installed="4096000"/>
  <location href="Packages/n/nginx-1.20.1-1.uos25.x86_64.rpm"/>
  <summary>High performance web server</summary>
  <description>A web server</description>
  <format>
    <rpm:license>BSD</rpm:license>
    <rpm:vendor>UOS</rpm:vendor>
    <rpm:group>Applications/System</rpm:group>
    <rpm:sourcerpm>nginx-1.20.1-1.uos25.src.rpm</rpm:sourcerpm>
    <rpm:provides>
      <rpm:entry name="webserver" flags="EQ" epoch="0" ver="1.20.1" rel="1.uos25"/>
      <rpm:entry name="/usr/sbin/nginx"/>
    </rpm:provides>
    <rpm:requires>
      <rpm:entry name="libc.so.6(GLIBC_2.14)" flags="GE" epoch="0" ver="2.14"/>
    </rpm:requires>
  </format>
</package>
</metadata>"#;

    #[test]
    fn test_parse_primary_xml_basic() {
        let pkgs = parse_primary_xml(PRIMARY_XML).unwrap();
        assert_eq!(pkgs.len(), 1);
        let p = &pkgs[0];
        assert_eq!(p.name, "nginx");
        assert_eq!(p.version, "1.20.1");
        assert_eq!(p.release, "1.uos25");
        // epoch=0 等价于无 epoch，不保留（与 dnf 显示语义一致）
        assert_eq!(p.epoch.as_deref(), None);
        assert_eq!(p.summary, "High performance web server");
        assert_eq!(p.size, Some(1024000));
        assert_eq!(p.install_size, Some(4096000));
        assert_eq!(p.build_time, Some(1690000000));
        assert_eq!(
            p.location.as_deref(),
            Some("Packages/n/nginx-1.20.1-1.uos25.x86_64.rpm")
        );
        assert_eq!(p.license.as_deref(), Some("BSD"));
        assert_eq!(
            p.source_pkg.as_deref(),
            Some("nginx-1.20.1-1.uos25.src.rpm")
        );
        assert_eq!(p.group.as_deref(), Some("Applications/System"));
    }

    #[test]
    fn test_parse_primary_xml_deps() {
        let pkgs = parse_primary_xml(PRIMARY_XML).unwrap();
        let p = &pkgs[0];
        assert!(p.provides.contains(&"webserver".to_string()));
        assert!(p.provides.contains(&"/usr/sbin/nginx".to_string()));
        assert_eq!(p.requires.len(), 1);
        assert_eq!(p.requires[0].name, "libc.so.6(GLIBC_2.14)");
        assert_eq!(p.requires[0].flags.as_deref(), Some("GE"));
        assert_eq!(p.requires[0].version.as_deref(), Some("2.14"));
    }

    #[test]
    fn test_parse_repomd_for_type() {
        let xml = r#"<?xml version="1.0"?>
<repomd xmlns="http://linux.duke.edu/metadata/repo">
  <data type="primary">
    <location href="repodata/primary.xml.gz"/>
    <timestamp>1690000000</timestamp>
  </data>
  <data type="filelists">
    <location href="repodata/filelists.xml.gz"/>
  </data>
</repomd>"#;
        assert_eq!(
            parse_repomd_for_type(xml, "primary").as_deref(),
            Some("repodata/primary.xml.gz")
        );
        assert_eq!(
            parse_repomd_for_type(xml, "filelists").as_deref(),
            Some("repodata/filelists.xml.gz")
        );
        assert_eq!(parse_repomd_for_type(xml, "other"), None);
    }

    #[test]
    fn test_parse_filelists_xml_stream() {
        let xml = r#"<?xml version="1.0"?>
<filelists xmlns="http://linux.duke.edu/metadata/filelists">
<package pkgid="abc" name="zip" arch="x86_64">
  <file>/usr/bin/zip</file>
  <file type="dir">/usr/share/zip</file>
  <file type="ghost">/var/lock/zip</file>
  <file>usr/share/man/man1/zip.1.gz</file>
</package>
<package pkgid="def" name="unzip" arch="x86_64">
  <file>/usr/bin/unzip</file>
</package>
</filelists>"#;
        let map = parse_filelists_xml_stream(std::io::Cursor::new(xml));
        assert_eq!(
            map["zip"],
            vec!["/usr/bin/zip", "/usr/share/man/man1/zip.1.gz"]
        );
        assert_eq!(map["unzip"], vec!["/usr/bin/unzip"]);
    }
}

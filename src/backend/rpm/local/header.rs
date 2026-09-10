//! RPM header 原生解析（NDB/BDB/SQLite 共用的 header blob 解码）。
//!
//! 从 `rpm/local.rs`（现为 `local/mod.rs`）拆出：TAG/TYPE 常量、`RpmHeaderData`、
//! header 索引扫描与字段提取。仅对 `rpm::local` 可见（`pub(super)`）。

use super::RPM_HEADER_MAGIC;
use crate::model::*;

/// 从 NAME(tag=1000) 索引项起精确测定索引条目数。
/// rpmdb 中 header 索引**并非全程严格递增**（实测尾部存在 5097→257 的回绕），
/// 故不能单调计数。策略：
/// 1. 逐条目做"合理性"检查（tag/type/offset/count 数值界限）确定上界 P；
/// 2. 从上界向下搜索 k，要求 data_start=idx+k*16 处名称合法，且 VERSION 条目
///    （entry1, tag=1001）指向的字符串含数字做交叉验证，首个通过者即真边界。
pub(super) fn count_ndb_index_entries(data: &[u8], name_entry: usize) -> usize {
    // 上界：连续合理条目数
    let mut plausible = 1usize;
    loop {
        let e = name_entry + plausible * 16;
        if e + 16 > data.len() {
            break;
        }
        let tag = u32::from_be_bytes([data[e], data[e + 1], data[e + 2], data[e + 3]]);
        let type_id = u32::from_be_bytes([data[e + 4], data[e + 5], data[e + 6], data[e + 7]]);
        let offset = u32::from_be_bytes([data[e + 8], data[e + 9], data[e + 10], data[e + 11]]);
        let count = u32::from_be_bytes([data[e + 12], data[e + 13], data[e + 14], data[e + 15]]);
        let ok = tag < 0x0010_0000 && type_id <= 9 && count < (1 << 24) && offset < (1 << 28);
        if !ok {
            break;
        }
        plausible += 1;
    }

    // VERSION 交叉验证信息：entry1
    let e1 = name_entry + 16;
    let (entry1_tag, entry1_off) = if e1 + 16 <= data.len() {
        (
            u32::from_be_bytes([data[e1], data[e1 + 1], data[e1 + 2], data[e1 + 3]]),
            u32::from_be_bytes([data[e1 + 8], data[e1 + 9], data[e1 + 10], data[e1 + 11]]) as usize,
        )
    } else {
        (0, 0)
    };

    let read_cstr = |pos: usize| -> Option<String> {
        if pos >= data.len() {
            return None;
        }
        let end = data[pos..].iter().position(|&b| b == 0)?;
        Some(String::from_utf8_lossy(&data[pos..pos + end]).to_string())
    };
    let is_name_like = |s: &str| -> bool {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.' || c == '_')
    };

    for k in (10..=plausible).rev() {
        let ds = name_entry + k * 16;
        if ds + 4 > data.len() {
            continue;
        }
        let name = match read_cstr(ds + 2) {
            Some(n) => n,
            None => continue,
        };
        if !is_name_like(&name) {
            continue;
        }
        // 交叉验证：VERSION 字符串应含数字
        if entry1_tag == 1001 {
            match read_cstr(ds + entry1_off).filter(|v| v.chars().any(|c| c.is_ascii_digit())) {
                Some(_) => return k,
                None => continue,
            }
        }
        return k;
    }
    0
}

pub(super) fn is_valid_pkg_name(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 200 {
        return false;
    }
    bytes.iter().all(|&b| {
        b.is_ascii_lowercase()
            || b.is_ascii_uppercase()
            || b.is_ascii_digit()
            || b == b'-'
            || b == b'+'
            || b == b'.'
            || b == b'_'
    }) && bytes.iter().any(|&b| b.is_ascii_alphabetic())
}

pub(super) const TAG_NAME: u32 = 1000;
pub(super) const TAG_VERSION: u32 = 1001;
pub(super) const TAG_RELEASE: u32 = 1002;
pub(super) const TAG_EPOCH: u32 = 1003;
pub(super) const TAG_SUMMARY: u32 = 1004;
pub(super) const TAG_DESCRIPTION: u32 = 1005;
pub(super) const TAG_BUILD_TIME: u32 = 1006;
pub(super) const TAG_SIZE: u32 = 1009;
pub(super) const TAG_INSTALL_SIZE: u32 = 1010;
pub(super) const TAG_VENDOR: u32 = 1011;
pub(super) const TAG_LICENSE: u32 = 1014;
pub(super) const TAG_PACKAGER: u32 = 1015;
pub(super) const TAG_GROUP: u32 = 1016;
pub(super) const TAG_URL: u32 = 1020;
pub(super) const TAG_ARCH: u32 = 1022;
pub(super) const TAG_SOURCERPM: u32 = 1044;
pub(super) const TAG_PROVIDENAME: u32 = 1047;
pub(super) const TAG_REQUIREFLAGS: u32 = 1048;
pub(super) const TAG_REQUIRENAME: u32 = 1049;
pub(super) const TAG_REQUIREVERSION: u32 = 1050;
pub(super) const TAG_CONFLICTNAME: u32 = 1054;
pub(super) const TAG_CONFLICTFLAGS: u32 = 1053;
pub(super) const TAG_CONFLICTVERSION: u32 = 1055;
pub(super) const TAG_OBSOLETENAME: u32 = 1090;
pub(super) const TAG_OBSOLETEFLAGS: u32 = 1114;
pub(super) const TAG_OBSOLETEVERSION: u32 = 1115;
pub(super) const TAG_BASENAMES: u32 = 1116;
pub(super) const TAG_DIRNAMES: u32 = 1117;
pub(super) const TAG_DIRINDEXES: u32 = 1118;
pub(super) const TAG_CHANGELOGTIME: u32 = 1080;
pub(super) const TAG_CHANGELOGNAME: u32 = 1081;
pub(super) const TAG_CHANGELOGTEXT: u32 = 1082;

pub(super) const TYPE_STRING: u32 = 6;
pub(super) const TYPE_STRING_ARRAY: u32 = 8;
pub(super) const TYPE_INT16: u32 = 3;
pub(super) const TYPE_INT32: u32 = 4;
pub(super) const TYPE_INT64: u32 = 5;
pub(super) const TYPE_I18NSTRING: u32 = 9;

pub(super) struct RpmHeaderData {
    entries: std::collections::HashMap<u32, (u32, usize, usize)>,
    data: Vec<u8>,
}

pub(super) fn parse_ndb_header(
    data: &[u8],
    idx_start: usize,
    nindex: usize,
    data_start: usize,
) -> Option<PkgMetadata> {
    let mut entries = std::collections::HashMap::new();

    for i in 0..nindex {
        let off = idx_start + i * 16;
        if off + 16 > data.len() {
            break;
        }
        let tag = u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
        let type_id =
            u32::from_be_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]);
        let offset =
            u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]])
                as usize;
        let count = u32::from_be_bytes([
            data[off + 12],
            data[off + 13],
            data[off + 14],
            data[off + 15],
        ]) as usize;
        entries.insert(tag, (type_id, offset, count));
    }

    let data_end = find_data_end(data, &entries, data_start, nindex);
    let header_data = data[data_start..data_end.min(data.len())].to_vec();

    let header = RpmHeaderData {
        entries,
        data: header_data,
    };

    build_pkg_metadata(&header)
}

pub(super) fn find_data_end(
    data: &[u8],
    entries: &std::collections::HashMap<u32, (u32, usize, usize)>,
    data_start: usize,
    nindex: usize,
) -> usize {
    let mut max_end = data_start;

    for &(type_id, offset, count) in entries.values() {
        let type_size = match type_id {
            0 => 0,
            1 => 1,
            2 => 1,
            3 => 2,
            4 => 4,
            5 => 8,
            6 => 0,
            7 => 1,
            8 => 0,
            9 => 0,
            _ => 0,
        };

        let abs_offset = data_start.saturating_add(offset);
        if abs_offset >= data.len() {
            continue;
        }

        if type_size == 0 {
            let start = abs_offset;
            let mut end = start;
            let mut remaining = count;
            while remaining > 0 && end < data.len() {
                if data[end] == 0 {
                    remaining -= 1;
                }
                end += 1;
            }
            if end > max_end {
                max_end = end;
            }
        } else {
            let end = abs_offset.saturating_add(count * type_size);
            if end > max_end && end <= data.len() {
                max_end = end;
            }
        }
    }

    let idx_end = data_start + nindex * 16;
    if idx_end > max_end {
        max_end = idx_end;
    }

    max_end
}

pub(super) fn build_pkg_metadata(header: &RpmHeaderData) -> Option<PkgMetadata> {
    let name = get_string(header, TAG_NAME)?;
    if name.is_empty() || name.len() < 2 || !is_valid_pkg_name(name.as_bytes()) {
        return None;
    }
    let version = get_string(header, TAG_VERSION).unwrap_or_default();
    let release = get_string(header, TAG_RELEASE).unwrap_or_default();
    if version.is_empty() || !version.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let arch = get_string(header, TAG_ARCH).unwrap_or_default();
    let summary = get_string(header, TAG_SUMMARY).unwrap_or_default();
    if summary.contains("#!/") || summary.contains("\n") {
        return None;
    }
    let description = get_string(header, TAG_DESCRIPTION).unwrap_or_default();

    let requires_names = get_string_array(header, TAG_REQUIRENAME);
    let requires_flags = get_int32_array(header, TAG_REQUIREFLAGS);
    let requires_versions = get_string_array(header, TAG_REQUIREVERSION);

    let requires: Vec<Dependency> = requires_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let flags = requires_flags.get(i).copied().unwrap_or(0);
            let ver = requires_versions.get(i).cloned();
            Dependency {
                name: name.clone(),
                version: ver.filter(|v| !v.is_empty()),
                flags: flags_to_string(flags),
                is_alternative: false,
            }
        })
        .filter(|d| {
            !d.name.starts_with("rpmlib(")
                && !d.name.starts_with("rtld(GNU_HASH)")
                && d.name != "libc.so.6"
                && d.name != "rtld(GNU_HASH)"
        })
        .collect();

    let provides = get_string_array(header, TAG_PROVIDENAME);

    let conflict_names = get_string_array(header, TAG_CONFLICTNAME);
    let conflict_flags = get_int32_array(header, TAG_CONFLICTFLAGS);
    let conflict_versions = get_string_array(header, TAG_CONFLICTVERSION);
    let conflicts: Vec<Dependency> = conflict_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let flags = conflict_flags.get(i).copied().unwrap_or(0);
            let ver = conflict_versions.get(i).cloned();
            Dependency {
                name: name.clone(),
                version: ver.filter(|v| !v.is_empty()),
                flags: flags_to_string(flags),
                is_alternative: false,
            }
        })
        .collect();

    let obsolete_names = get_string_array(header, TAG_OBSOLETENAME);
    let obsolete_flags = get_int32_array(header, TAG_OBSOLETEFLAGS);
    let obsolete_versions = get_string_array(header, TAG_OBSOLETEVERSION);
    let obsoletes: Vec<Dependency> = obsolete_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let flags = obsolete_flags.get(i).copied().unwrap_or(0);
            let ver = obsolete_versions.get(i).cloned();
            Dependency {
                name: name.clone(),
                version: ver.filter(|v| !v.is_empty()),
                flags: flags_to_string(flags),
                is_alternative: false,
            }
        })
        .collect();

    let files = extract_files_from_header(header);

    let changelog = extract_changelog(header);

    Some(PkgMetadata {
        name,
        version,
        release,
        epoch: get_int32(header, TAG_EPOCH).map(|e| e.to_string()),
        arch,
        summary,
        description,
        url: get_string(header, TAG_URL),
        license: get_string(header, TAG_LICENSE),
        vendor: get_string(header, TAG_VENDOR),
        packager: get_string(header, TAG_PACKAGER),
        source_pkg: get_string(header, TAG_SOURCERPM),
        size: None,
        install_size: get_int32(header, TAG_INSTALL_SIZE)
            .or_else(|| get_int32(header, TAG_SIZE))
            .map(|v| v as u64),
        group: get_string(header, TAG_GROUP),
        priority: None,
        build_time: get_int32(header, TAG_BUILD_TIME),
        location: None,
        source_repo: None,
        requires,
        recommends: Vec::new(),
        suggests: Vec::new(),
        provides,
        conflicts,
        obsoletes,
        replaces: Vec::new(),
        files,
        changelog,
    })
}

pub(super) fn extract_changelog(header: &RpmHeaderData) -> Vec<ChangelogEntry> {
    let times = get_int32_array(header, TAG_CHANGELOGTIME);
    let names = get_string_array(header, TAG_CHANGELOGNAME);
    let texts = get_string_array(header, TAG_CHANGELOGTEXT);

    let mut entries = Vec::new();
    for ((t, n), x) in times.iter().zip(names.iter()).zip(texts.iter()) {
        entries.push(ChangelogEntry {
            author: n.clone(),
            timestamp: *t as i64,
            text: x.clone(),
        });
    }

    entries
}

pub(super) fn parse_rpm_header(data: &[u8]) -> Option<PkgMetadata> {
    let header = parse_rpm_header_raw(data)?;
    build_pkg_metadata(&header)
}

pub(super) fn parse_rpm_header_raw(data: &[u8]) -> Option<RpmHeaderData> {
    if data.len() < 16 {
        return None;
    }
    if data[0..4] != RPM_HEADER_MAGIC {
        return None;
    }

    let reserved = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if reserved != 0 {
        return None;
    }

    let nindex = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let hsize = u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize;

    let index_start = 16;
    let index_end = index_start + nindex * 16;
    let data_start = index_end;
    let data_end = data_start + hsize;

    if data_end > data.len() {
        return None;
    }

    let mut entries = std::collections::HashMap::new();
    for i in 0..nindex {
        let off = index_start + i * 16;
        let tag = u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
        let type_id =
            u32::from_be_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]);
        let offset =
            u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]])
                as usize;
        let count = u32::from_be_bytes([
            data[off + 12],
            data[off + 13],
            data[off + 14],
            data[off + 15],
        ]) as usize;
        entries.insert(tag, (type_id, offset, count));
    }

    let header_data = data[data_start..data_end].to_vec();
    Some(RpmHeaderData {
        entries,
        data: header_data,
    })
}

pub(super) fn get_string(header: &RpmHeaderData, tag: u32) -> Option<String> {
    let &(type_id, offset, count) = header.entries.get(&tag)?;
    if count == 0 {
        return None;
    }
    match type_id {
        TYPE_STRING | TYPE_STRING_ARRAY | TYPE_I18NSTRING => {
            if offset >= header.data.len() {
                return None;
            }
            let data = &header.data[offset..];
            let end = data.iter().position(|&b| b == 0)?;
            Some(String::from_utf8_lossy(&data[..end]).to_string())
        }
        _ => None,
    }
}

pub(super) fn get_string_array(header: &RpmHeaderData, tag: u32) -> Vec<String> {
    let (_type_id, offset, count) = match header.entries.get(&tag) {
        Some(&v) => v,
        None => return Vec::new(),
    };
    if offset >= header.data.len() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let data = &header.data[offset..];
    let mut start = 0;
    let mut remaining = count;
    for (i, &b) in data.iter().enumerate() {
        if b == 0 {
            if remaining == 0 {
                break;
            }
            result.push(String::from_utf8_lossy(&data[start..i]).to_string());
            remaining -= 1;
            start = i + 1;
        }
    }
    result
}

pub(super) fn get_int32(header: &RpmHeaderData, tag: u32) -> Option<i64> {
    let &(type_id, offset, count) = header.entries.get(&tag)?;
    if count == 0 {
        return None;
    }
    match type_id {
        TYPE_INT32 => {
            if offset + 4 > header.data.len() {
                return None;
            }
            Some(i32::from_be_bytes([
                header.data[offset],
                header.data[offset + 1],
                header.data[offset + 2],
                header.data[offset + 3],
            ]) as i64)
        }
        TYPE_INT64 => {
            if offset + 8 > header.data.len() {
                return None;
            }
            Some(i64::from_be_bytes([
                header.data[offset],
                header.data[offset + 1],
                header.data[offset + 2],
                header.data[offset + 3],
                header.data[offset + 4],
                header.data[offset + 5],
                header.data[offset + 6],
                header.data[offset + 7],
            ]))
        }
        _ => None,
    }
}

pub(super) fn get_int32_array(header: &RpmHeaderData, tag: u32) -> Vec<i32> {
    let (type_id, offset, count) = match header.entries.get(&tag) {
        Some(&v) => v,
        None => return Vec::new(),
    };
    match type_id {
        TYPE_INT32 => {
            let mut result = Vec::new();
            for i in 0..count {
                let pos = offset + i * 4;
                if pos + 4 <= header.data.len() {
                    result.push(i32::from_be_bytes([
                        header.data[pos],
                        header.data[pos + 1],
                        header.data[pos + 2],
                        header.data[pos + 3],
                    ]));
                }
            }
            result
        }
        TYPE_INT16 => {
            let mut result = Vec::new();
            for i in 0..count {
                let pos = offset + i * 2;
                if pos + 2 <= header.data.len() {
                    result
                        .push(i16::from_be_bytes([header.data[pos], header.data[pos + 1]]) as i32);
                }
            }
            result
        }
        _ => Vec::new(),
    }
}

/// 将 RPM 依赖标志位解析为标准比较运算符字符串。
/// RPMSENSE_LESS=0x02, RPMSENSE_GREATER=0x04, RPMSENSE_EQUAL=0x08
pub(super) fn flags_to_string(flags: i32) -> Option<String> {
    if flags == 0 {
        return None;
    }
    let f = flags as u32;
    let is_less = (f & 0x02) != 0;
    let is_greater = (f & 0x04) != 0;
    let is_equal = (f & 0x08) != 0;
    match (is_less, is_greater, is_equal) {
        (true, false, true) => Some("<=".to_string()),
        (false, true, true) => Some(">=".to_string()),
        (false, false, true) => Some("=".to_string()),
        (true, false, false) => Some("<".to_string()),
        (false, true, false) => Some(">".to_string()),
        _ => None,
    }
}

pub(super) fn extract_files_from_header(header: &RpmHeaderData) -> Vec<String> {
    let s1116 = get_string_array(header, TAG_BASENAMES);
    let s1117 = get_string_array(header, TAG_DIRNAMES);
    let s1118 = get_string_array(header, TAG_DIRINDEXES);
    let i1116 = get_int32_array(header, TAG_BASENAMES);
    let i1118 = get_int32_array(header, TAG_DIRINDEXES);

    // 标准 RPM 规范：BASENAMES(1116)=字符串[N]、DIRNAMES(1117)=字符串[M]、DIRINDEXES(1118)=int32[N]。
    // 实测部分 rpmdb（UOS Server 25 / rpm 4.18 ndb）三者内容错位：
    //   1116=int32[N]（实为 dirindexes）、1117=字符串[N]（basenames）、1118=字符串[M]（dirnames）。
    // 按 (offset,count) 语义嗅探兼容两种布局：
    //   1) 整型数组 = dirindexes（优先 1118，其次 1116）
    //   2) 与 dirindexes 等长的字符串数组 = basenames；另一个字符串数组 = dirnames
    let (dirindexes, str_x, str_y) = if !i1118.is_empty() {
        (i1118, s1116, s1117)
    } else if !i1116.is_empty() {
        (i1116, s1117, s1118)
    } else {
        return Vec::new();
    };

    let (basenames, dirnames) = if str_x.len() == dirindexes.len() && str_x.len() >= str_y.len() {
        (str_x, str_y)
    } else if str_y.len() == dirindexes.len() {
        (str_y, str_x)
    } else {
        (str_x, str_y)
    };

    let mut files = Vec::new();
    for (i, base) in basenames.iter().enumerate() {
        if let Some(&dir_idx) = dirindexes.get(i) {
            if let Some(dir) = dirnames.get(dir_idx as usize) {
                files.push(format!("{}{}", dir, base));
            }
        }
    }
    files
}

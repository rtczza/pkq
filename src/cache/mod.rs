use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{PkgError, Result};
use crate::model::PkgMetadata;

const CACHE_VERSION: u32 = 6;

#[derive(Serialize, Deserialize)]
pub struct PkgIndexCache {
    pub version: u32,
    pub packages: Vec<PkgMetadata>,
}

fn cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("pkq");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// 缓存占用清单（相对路径, 字节数），按大小降序
pub fn cache_status() -> Vec<(String, u64)> {
    let base = cache_dir();
    let mut items: Vec<(String, u64)> = Vec::new();
    fn dir_size(dir: &Path) -> u64 {
        let mut total = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    total += dir_size(&p);
                } else if let Ok(m) = e.metadata() {
                    total += m.len();
                }
            }
        }
        total
    }
    if let Ok(entries) = std::fs::read_dir(&base) {
        for e in entries.flatten() {
            let p = e.path();
            let size = if p.is_dir() {
                dir_size(&p)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            };
            if size > 0 {
                let rel = p
                    .strip_prefix(&base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .to_string();
                items.push((rel, size));
            }
        }
    }
    items.sort_by(|a, b| b.1.cmp(&a.1));
    items
}

/// pkq 缓存目录树中的最大 mtime（即最近一次元数据下载/落盘时间）。
/// 用于「上次元数据过期检查」Banner 的真实时间源——优先于发行版原生缓存目录。
pub fn cache_dir_mtime() -> u64 {
    fn walk(dir: &Path) -> u64 {
        let mut max = 0u64;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    max = max.max(walk(&p));
                } else if let Ok(m) = e.metadata() {
                    if let Ok(t) = m.modified() {
                        let s = t
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if s > max {
                            max = s;
                        }
                    }
                }
            }
        }
        max
    }
    walk(&cache_dir())
}

/// 按清理目标计算当前占用字节数（用于 clean 前的确认提示）。
/// target: all | index | repos | contents
pub fn cache_target_size(target: &str) -> u64 {
    let base = cache_dir();
    fn dir_size(dir: &Path) -> u64 {
        let mut total = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    total += dir_size(&p);
                } else if let Ok(m) = e.metadata() {
                    total += m.len();
                }
            }
        }
        total
    }
    match target {
        "all" => dir_size(&base),
        "index" => dir_size(&base.join("index")),
        "repos" => dir_size(&base.join("repos")),
        "contents" => [deb_contents_raw_cache_path(), deb_contents_raw_meta_path()]
            .iter()
            .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .sum(),
        _ => 0,
    }
}

/// 清理缓存，返回释放的字节数。target: all | index | repos | contents
pub fn cache_clean(target: &str) -> Result<u64> {
    let base = cache_dir();
    let before = cache_target_size(target);
    let remove = |p: &Path| -> Result<()> {
        if p.is_dir() {
            std::fs::remove_dir_all(p)
        } else if p.exists() {
            std::fs::remove_file(p)
        } else {
            Ok(())
        }
        .map_err(|e| PkgError::IoError(format!("清理失败 {:?}: {}", p, e)))
    };
    match target {
        "all" => remove(&base)?,
        "index" => remove(&base.join("index"))?,
        "repos" => remove(&base.join("repos"))?,
        "contents" => {
            remove(&deb_contents_raw_cache_path())?;
            remove(&deb_contents_raw_meta_path())?;
        }
        other => {
            return Err(PkgError::InvalidArgument(format!(
                "未知清理目标: {}（可选 all|index|repos|contents）",
                other
            )))
        }
    }
    let after: u64 = cache_status().iter().map(|(_, s)| *s).sum();
    Ok(before.saturating_sub(after))
}

/// 原子落盘前确保目标父目录存在（改名迁移回归修复：
/// cache_dir 不再预建 index 子目录，save 必须自建，否则静默失败）。
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            PkgError::IoError(format!("Failed to create cache dir {:?}: {}", parent, e))
        })?;
    }
    Ok(())
}

/// 缓存文件魔数：与历史 bincode 1.x 格式强制隔离。
/// 历史事故：bincode 1 数据被 postcard varint 误读为「0 个包的合法缓存」
/// （version 字段恰好通过校验），导致本地库静默失效。魔数使异构格式
/// 在解析前即被拒绝。
const CACHE_MAGIC: &[u8; 4] = b"PKQ1";

fn encode_cache<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let body = postcard::to_allocvec(value)
        .map_err(|e| PkgError::IoError(format!("Failed to serialize cache: {}", e)))?;
    let mut buf = Vec::with_capacity(body.len() + CACHE_MAGIC.len());
    buf.extend_from_slice(CACHE_MAGIC);
    buf.extend_from_slice(&body);
    Ok(buf)
}

fn decode_cache<T: serde::de::DeserializeOwned>(data: &[u8]) -> Option<T> {
    let body = data.get(CACHE_MAGIC.len()..)?;
    if &data[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return None; // 异构/损坏格式：拒绝解析
    }
    postcard::from_bytes(body).ok()
}

fn file_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn max_source_mtime(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|p| {
            let m = file_mtime(p);
            if m > 0 {
                Some(m)
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
}

impl PkgIndexCache {
    pub fn load(path: &Path, ttl: u64, force: bool, source_files: &[PathBuf]) -> Option<Self> {
        if force {
            return None;
        }
        let cache_mtime = file_mtime(path);
        if cache_mtime == 0 {
            return None;
        }
        let src_max = max_source_mtime(source_files);
        if cache_mtime < src_max {
            return None;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if ttl > 0 && now > cache_mtime && now - cache_mtime > ttl {
            return None;
        }
        let data = std::fs::read(path).ok()?;
        let cache: PkgIndexCache = decode_cache(&data)?;
        if cache.version != CACHE_VERSION {
            return None;
        }
        // 空包列表 = 损坏缓存（历史事故：异构格式被误读为 0 个包）
        if cache.packages.is_empty() {
            return None;
        }
        Some(cache)
    }

    pub fn save(path: &Path, packages: Vec<PkgMetadata>) -> Result<()> {
        let cache = PkgIndexCache {
            version: CACHE_VERSION,
            packages,
        };
        let data = encode_cache(&cache)?;
        let tmp = path.with_extension("tmp");
        ensure_parent_dir(path)?;
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct ContentsMeta {
    version: u32,
}

impl ContentsMeta {
    pub fn check_valid(meta_path: &Path, ttl: u64, force: bool, source_files: &[PathBuf]) -> bool {
        if force {
            return false;
        }
        let meta_mtime = file_mtime(meta_path);
        if meta_mtime == 0 {
            return false;
        }
        let src_max = max_source_mtime(source_files);
        if meta_mtime < src_max {
            return false;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if ttl > 0 && now > meta_mtime && now - meta_mtime > ttl {
            return false;
        }
        let data = match std::fs::read(meta_path) {
            Ok(d) => d,
            Err(_) => return false,
        };
        let meta: ContentsMeta = match decode_cache(&data) {
            Some(m) => m,
            None => return false,
        };
        meta.version == CACHE_VERSION
    }

    pub fn save_meta(path: &Path) -> Result<()> {
        let meta = ContentsMeta {
            version: CACHE_VERSION,
        };
        let data = encode_cache(&meta)?;
        let tmp = path.with_extension("tmp");
        ensure_parent_dir(path)?;
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn deb_cache_path() -> PathBuf {
    cache_dir().join("index").join("deb_packages.bin")
}

/// RPM 本地 rpmdb 解析结果缓存（P1-1）
pub fn rpm_local_cache_path() -> PathBuf {
    cache_dir().join("index").join("rpm_local.bin")
}

/// RPM 仓库 primary 解析结果缓存（P1-1），按 repo_id + baseurl 区分
pub fn rpm_repo_cache_path(repo_id: &str, baseurl: &str) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    baseurl.hash(&mut hasher);
    let safe_id: String = repo_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    cache_dir()
        .join("index")
        .join(format!("rpm_repo_{}_{:016x}.bin", safe_id, hasher.finish()))
}

/// RPM 仓库 primary 解析缓存：以 repomd 中 primary 数据项的 timestamp 为失效键
#[derive(Serialize, Deserialize)]
pub struct RpmRepoCache {
    pub primary_ts: i64,
    pub packages: Vec<PkgMetadata>,
}

impl RpmRepoCache {
    pub fn load(path: &Path, expected_ts: i64) -> Option<Self> {
        if expected_ts <= 0 {
            return None;
        }
        let data = std::fs::read(path).ok()?;
        let cache: RpmRepoCache = decode_cache(&data)?;
        if cache.primary_ts != expected_ts {
            return None;
        }
        // 空包列表 = 损坏缓存
        if cache.packages.is_empty() {
            return None;
        }
        Some(cache)
    }

    /// repomd 不可用时的最后降级：忽略时间戳校验直接加载解析缓存，
    /// 数据可能过期但保证仓库查询在离线/源故障场景下仍可用
    pub fn load_any(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        let cache: RpmRepoCache = decode_cache(&data)?;
        // 空包列表 = 损坏缓存
        if cache.packages.is_empty() {
            return None;
        }
        Some(cache)
    }

    pub fn save(path: &Path, primary_ts: i64, packages: Vec<PkgMetadata>) -> Result<()> {
        let cache = RpmRepoCache {
            primary_ts,
            packages,
        };
        let data = encode_cache(&cache)?;
        let tmp = path.with_extension("tmp");
        ensure_parent_dir(path)?;
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn deb_contents_raw_cache_path() -> PathBuf {
    cache_dir().join("index").join("deb_contents_raw.txt")
}

pub fn deb_contents_raw_meta_path() -> PathBuf {
    cache_dir().join("index").join("deb_contents_raw.meta")
}

/// RPM filelists 扁平索引（`file_path\tpkg_name\n`），落盘后 mmap 检索（对齐 DEB Contents）。
pub fn rpm_filelists_cache_path() -> PathBuf {
    cache_dir().join("index").join("rpm_filelists.txt")
}

/// RPM filelists 扁平索引的失效键元数据。
pub fn rpm_filelists_meta_path() -> PathBuf {
    cache_dir().join("index").join("rpm_filelists.meta")
}

/// RPM filelists 索引的失效键（各源 filelists 数据项时间戳，按仓库顺序拼接）。
#[derive(Serialize, Deserialize)]
pub struct RpmFilelistsMeta {
    pub key: String,
}

impl RpmFilelistsMeta {
    pub fn load(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        decode_cache(&data)
    }

    pub fn save(path: &Path, key: String) -> Result<()> {
        let data = encode_cache(&RpmFilelistsMeta { key })?;
        let tmp = path.with_extension("tmp");
        ensure_parent_dir(path)?;
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

pub fn deb_apt_sources() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/etc/apt/sources.list")];
    let sources_d = PathBuf::from("/etc/apt/sources.list.d");
    if let Ok(entries) = std::fs::read_dir(&sources_d) {
        for entry in entries.flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str());
            // .list（one-line）与 .sources（deb822）均为有效源定义
            if ext == Some("list") || ext == Some("sources") {
                paths.push(p);
            }
        }
    }
    paths
}

pub fn deb_apt_lists_packages() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let lists_dir = PathBuf::from("/var/lib/apt/lists");
    if let Ok(entries) = std::fs::read_dir(&lists_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.to_string_lossy().ends_with("_Packages") {
                paths.push(p);
            }
        }
    }
    paths
}

pub fn deb_apt_lists_contents() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let lists_dir = PathBuf::from("/var/lib/apt/lists");
    if let Ok(entries) = std::fs::read_dir(&lists_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.to_string_lossy();
            if name.contains("Contents-") && name.ends_with(".lz4") {
                paths.push(p);
            }
        }
    }
    paths
}

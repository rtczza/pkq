use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::{PkgError, Result};

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize)]
struct HttpMeta {
    etag: Option<String>,
    last_modified: Option<String>,
}

/// 一次元数据抓取请求：收敛原先 7 个位置参数（消除 `clippy::too_many_arguments`）。
#[derive(Debug, Clone, Copy)]
pub struct FetchRequest<'a> {
    pub repo_id: &'a str,
    pub url: &'a str,
    pub ttl: Duration,
    pub force: bool,
    pub offline: bool,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
}

/// 仓库级查询参数：同一仓库下的多个 URL 共用策略与凭据。
#[derive(Debug, Clone, Copy)]
pub struct RepoQuery<'a> {
    pub repo_id: &'a str,
    pub base_url: &'a str,
    pub ttl: Duration,
    pub force: bool,
    pub offline: bool,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
}

impl<'a> RepoQuery<'a> {
    /// 为同源的具体 URL 生成抓取请求。
    pub fn request<'u>(&'u self, url: &'u str) -> FetchRequest<'u>
    where
        'a: 'u,
    {
        FetchRequest {
            repo_id: self.repo_id,
            url,
            ttl: self.ttl,
            force: self.force,
            offline: self.offline,
            username: self.username,
            password: self.password,
        }
    }
}

pub struct NetworkCacheManager {
    cache_dir: PathBuf,
}

impl NetworkCacheManager {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("pkq")
            .join("repos");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!("Failed to create cache dir {:?}: {}", cache_dir, e);
        }
        Self { cache_dir }
    }

    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!("Failed to create cache dir {:?}: {}", cache_dir, e);
        }
        Self { cache_dir }
    }

    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    pub fn fetch_index(&self, req: FetchRequest<'_>) -> Result<PathBuf> {
        let FetchRequest {
            repo_id,
            url,
            ttl,
            force: force_refresh,
            offline: offline_mode,
            username,
            password,
        } = req;

        let repo_dir = self.cache_dir.join(repo_id);
        if let Err(e) = std::fs::create_dir_all(&repo_dir) {
            tracing::warn!("Failed to create repo dir {:?}: {}", repo_dir, e);
        }

        let local_path = repo_dir.join(sanitize_filename(url));
        let meta_path = local_path.with_extension("meta.json");

        if offline_mode {
            if local_path.exists() {
                tracing::debug!("cache hit (offline): {}", url);
                return Ok(local_path);
            }
            return Err(PkgError::NetworkError(format!(
                "Offline mode: no cached copy of {}",
                url
            )));
        }

        if !force_refresh && local_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&local_path) {
                if let Ok(modified) = metadata.modified() {
                    if SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default()
                        < ttl
                    {
                        tracing::debug!("cache hit (ttl): {}", url);
                        return Ok(local_path);
                    }
                }
            }
        }

        let mut request = ureq::get(url).timeout(HTTP_TIMEOUT);

        if let (Some(user), Some(pass)) = (username, password) {
            if is_insecure_credentialed(url, true) {
                tracing::warn!(
                    "Warning: sending credentials over non-HTTPS URL ({}) — \
                     use https:// or remove the credentials",
                    url
                );
            }
            let credentials = format!("{}:{}", user, pass);
            let encoded = base64_encode(&credentials);
            request = request.set("Authorization", &format!("Basic {}", encoded));
        }

        let http_meta = load_meta(&meta_path);
        if let Some(ref meta) = http_meta {
            if let Some(ref etag) = meta.etag {
                request = request.set("If-None-Match", etag);
            }
            if let Some(ref lm) = meta.last_modified {
                request = request.set("If-Modified-Since", lm);
            }
        }

        match request.call() {
            Ok(response) => {
                let etag = response.header("ETag").map(|s| s.to_string());
                let last_modified = response.header("Last-Modified").map(|s| s.to_string());

                let mut reader = response.into_reader();
                let tmp_path = local_path.with_extension("tmp");
                {
                    let mut out_file = std::fs::File::create(&tmp_path)?;
                    std::io::copy(&mut reader, &mut out_file)?;
                    out_file.sync_all()?;
                }
                // 防护：0 字节响应（连接早断/服务器异常）不得覆盖完好缓存，
                // 否则 stale 回退会读到空文件并永久破坏缓存
                if std::fs::metadata(&tmp_path)
                    .map(|m| m.len() == 0)
                    .unwrap_or(true)
                {
                    let _ = std::fs::remove_file(&tmp_path);
                    if !local_path.exists() {
                        return Err(PkgError::NetworkError(format!(
                            "Empty response body from {}",
                            url
                        )));
                    }
                    return Ok(local_path);
                }
                std::fs::rename(&tmp_path, &local_path)?;
                tracing::debug!("fetched metadata: {}", url);

                let new_meta = HttpMeta {
                    etag,
                    last_modified,
                };
                let _ = std::fs::write(&meta_path, serde_json::to_string(&new_meta)?);

                Ok(local_path)
            }
            Err(ureq::Error::Status(304, _)) => {
                let _ = filetime::set_file_mtime(&local_path, filetime::FileTime::now());
                Ok(local_path)
            }
            Err(e) => {
                if local_path.exists() {
                    return Ok(local_path);
                }
                Err(PkgError::NetworkError(format!(
                    "Failed to fetch {}: {}",
                    url, e
                )))
            }
        }
    }

    pub fn fetch_text(&self, req: FetchRequest<'_>) -> Result<String> {
        let path = self.fetch_index(req)?;
        std::fs::read_to_string(&path)
            .map_err(|e| PkgError::IoError(format!("Failed to read cached file {:?}: {}", path, e)))
    }
}

impl Default for NetworkCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

fn load_meta(meta_path: &PathBuf) -> Option<HttpMeta> {
    let content = std::fs::read_to_string(meta_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 判断一次请求是否会在非 HTTPS 上发送凭据（Basic 认证）。
/// 明文传输凭据存在中间人窃取风险，调用方应据此告警或拒绝。
fn is_insecure_credentialed(url: &str, has_credentials: bool) -> bool {
    has_credentials && !url.starts_with("https://")
}

fn sanitize_filename(url: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();

    let cleaned = url
        .replace("://", "_")
        .replace('/', "_")
        .replace("?", "_")
        .replace("&", "_")
        .replace("=", "_");
    let prefix: String = cleaned.chars().take(150).collect();
    format!("{}_{}", prefix, hash)
}

fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        let name1 = sanitize_filename("https://example.com/path/file.xml");
        let name2 = sanitize_filename("https://example.com/path_file.xml");
        assert_ne!(name1, name2);
        assert!(name1.starts_with("https_example.com_path_file.xml_"));
    }

    #[test]
    fn test_cache_dir_creation() {
        let dir = std::env::temp_dir().join("pkq_test_cache");
        let _mgr = NetworkCacheManager::with_cache_dir(dir.clone());
        assert!(dir.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_offline_mode_no_cache() {
        let dir = std::env::temp_dir().join("pkq_test_offline");
        std::fs::remove_dir_all(&dir).ok();
        let mgr = NetworkCacheManager::with_cache_dir(dir);
        let result = mgr.fetch_index(FetchRequest {
            repo_id: "test",
            url: "https://nonexistent.example.com/test.xml",
            ttl: Duration::from_secs(3600),
            force: false,
            offline: true,
            username: None,
            password: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_insecure_credentialed_detection() {
        // 凭据 + 非 HTTPS = 明文风险
        assert!(is_insecure_credentialed("http://repo.example.com/x", true));
        // HTTPS 安全
        assert!(!is_insecure_credentialed(
            "https://repo.example.com/x",
            true
        ));
        // 无凭据不受影响
        assert!(!is_insecure_credentialed(
            "http://repo.example.com/x",
            false
        ));
    }

    #[test]
    fn test_base64_encode() {
        // RFC 4648 测试向量
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("f"), "Zg==");
        assert_eq!(base64_encode("fo"), "Zm8=");
        assert_eq!(base64_encode("foo"), "Zm9v");
        assert_eq!(base64_encode("foob"), "Zm9vYg==");
        assert_eq!(base64_encode("fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode("foobar"), "Zm9vYmFy");
        // Basic Auth 典型用例
        assert_eq!(base64_encode("user:password"), "dXNlcjpwYXNzd29yZA==");
    }
}

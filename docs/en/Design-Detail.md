# pkq Design Detail (Network Sync & Cache Module)

> **Archive notice**: this document is a stage-evolution design record. Code
> snippets are design-time drafts; the final implementation lives in the
> repository under `src/` (`network/cache.rs`, `backend/`) and the actual
> subcommands are defined in `src/cli.rs` (nine subcommands:
> `info/list/owns/deps/rdeps/search/source/changelog/cache`).

## 1. Document Goal

This document details the design of the Network Sync & Cache Management module
(`NetworkCacheManager`) and how the RPM/DEB adapters fetch and parse online
repository indexes over native HTTP.

## 2. Stage Acceptance Criteria

- [x] Network module spec: ureq-based requests, HTTP header conditional checks,
  timeout and stale-fallback handling.

- [x] Clear cache directory layout: `~/.cache/pkq/repos/` organization and
  metadata timestamp management.

- [x] Smooth adapter online parsing: RPM fetches `repomd.xml -> primary.xml.gz`
  in real time; DEB fetches `InRelease -> Packages.gz`.

## 3. Detailed Module Design

### 3.1 Network Sync & Cache Management Module (`NetworkCacheManager`)

```rust
// src/network/cache.rs
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use ureq;

pub struct NetworkCacheManager {
    cache_dir: PathBuf, // ~/.cache/pkq/repos/
}

impl NetworkCacheManager {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("pkq")
            .join("repos");
        std::fs::create_dir_all(&cache_dir).ok();
        Self { cache_dir }
    }

    /// Fetch the remote URL's metadata file (with ETag validation and TTL)
    pub fn fetch_index(&self, repo_id: &str, url: &str, ttl: Duration, force_refresh: bool) -> Result<PathBuf, PkgError> {
        let local_path = self.cache_dir.join(repo_id).join(self.sanitize_filename(url));
        let meta_path = local_path.with_extension("meta.json");

        // 1. Check whether the local cache is valid and not expired
        if !force_refresh && local_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&local_path) {
                if let Ok(modified) = metadata.modified() {
                    if SystemTime::now().duration_since(modified).unwrap_or_default() < ttl {
                        return Ok(local_path); // cache hit
                    }
                }
            }
        }

        // 2. Issue a native HTTP GET / HEAD request
        let mut request = ureq::get(url).timeout(Duration::from_secs(10));

        // Attach ETag / If-Modified-Since headers when a meta record exists
        if let Ok(meta_content) = std::fs::read_to_string(&meta_path) {
            if let Ok(json) = serde_json::from_str::<HttpMeta>(&meta_content) {
                if let Some(etag) = json.etag {
                    request = request.set("If-None-Match", &etag);
                }
            }
        }

        let response = match request.call() {
            Ok(res) => res,
            Err(ureq::Error::Status(304, _)) => {
                // 304 Not Modified: refresh file mtime and return the local path
                let _ = filetime::set_file_mtime(&local_path, filetime::FileTime::now());
                return Ok(local_path);
            }
            Err(e) => {
                // Network failure fallback: use the stale cache if it exists
                if local_path.exists() {
                    eprintln!("Warning: Network error ({}), fallback to stale cache.", e);
                    return Ok(local_path);
                }
                return Err(PkgError::NetworkError(format!("Failed to fetch {}: {}", url, e)));
            }
        };

        // 3. Write the file and record HTTP response header metadata
        let etag = response.header("ETag").map(|s| s.to_string());
        let mut reader = response.into_reader();
        let mut out_file = std::fs::File::create(&local_path)?;
        std::io::copy(&mut reader, &mut out_file)?;

        // Persist the ETag metadata
        let http_meta = HttpMeta { etag };
        let _ = std::fs::write(meta_path, serde_json::to_string(&http_meta)?);

        Ok(local_path)
    }

    fn sanitize_filename(&self, url: &str) -> String {
        // Turn a URL into a safe cache filename
        url.replace("://", "_").replace('/', "_")
    }
}

#[derive(Serialize, Deserialize)]
struct HttpMeta {
    etag: Option<String>,
}
```

### 3.2 RPM Adapter Online Workflow

1. Parse `/etc/yum.repos.d/*.repo`: obtain the remote baseurl (e.g.
   `https://mirrors.kernel.org/fedora/releases/39/Everything/x86_64/os/`).
2. Request `repomd.xml`: call
   `NetworkCacheManager.fetch_index(repo_id, "https://.../repodata/repomd.xml", ttl, force)`.
3. Locate and request `primary.xml.gz`: parse the newest `primary.xml.gz`
   relative path from repomd.xml, download it over HTTP and cache it.
4. Parse the metadata: use `flate2` + `quick-xml` to read the cached
   `.xml.gz` and assemble `PkgMetadata`.

### 3.3 DEB Adapter Online Workflow

1. Parse `/etc/apt/sources.list`: extract the Base URL, distribution codename
   (e.g. `jammy`) and component (`main`).
2. Request `Packages.gz`: the network module assembles the URL
   (e.g.
   `http://archive.ubuntu.com/ubuntu/dists/jammy/main/binary-amd64/Packages.gz`)
   and downloads it over HTTP.
3. Parse the text: stream-decompress the Packages control text and load the
   metadata.

### 3.4 CLI Network Control Flags

Global network/cache control flags added to the `Cli` struct:

```rust
#[derive(Parser)]
#[command(name = "pkq", author, version, about = "Native Linux Package Inspection Tool")]
pub struct Cli {
    #[arg(short, long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub output: OutputFormat,

    #[arg(long, global = true, help = "Force refresh online repository metadata cache via network")]
    pub refresh: bool,

    #[arg(long, global = true, help = "Offline mode: strictly prohibit network IO and use local cache only")]
    pub offline: bool,

    #[arg(long, global = true, default_value_t = 86400, help = "Cache TTL in seconds (default: 86400s / 24h)")]
    pub cache_ttl: u64,

    #[command(subcommand)]
    pub command: Commands,
}
```

### 3.5 CLI Subcommand Definition (early draft)

```rust
#[derive(Subcommand)]
pub enum Commands {
    /// Query package info (name, version, description, size, arch, license, etc.)
    Info { name: String, #[arg(long)] repo: bool },
    /// List files shipped in a package
    ListFiles { name: String, #[arg(long)] repo: bool },
    /// Reverse lookup: find the owning package of a file path
    OwnsFile { path: String, #[arg(long)] repo: bool },
    /// Show forward dependencies of a package
    Deps { name: String, #[arg(long)] repo: bool },
    /// Show reverse dependencies (which packages depend on this one)
    RDeps { name: String, #[arg(long)] repo: bool },
    /// Search packages by name/description
    Search { keyword: String, #[arg(long)] repo: bool },
    /// List all installed packages
    ListInstalled,
    /// List all available packages in configured repositories
    ListRepo,
    /// Show raw metadata fields of a package
    Metadata { name: String, #[arg(long)] repo: bool },
    /// Show the package change log
    Changelog { name: String, #[arg(long)] repo: bool },
    /// Source/binary package lookup
    Source {
        name: String,
        #[arg(long, help = "Reverse lookup: find binary packages by source package name")]
        from_source: bool,
        #[arg(long)]
        repo: bool,
    },
    /// Generate download command text (by package name or --file path)
    GenerateDownloadCmd {
        name: Option<String>,
        #[arg(long, help = "Resolve the owning package of a file path, then generate the download command")]
        file: Option<String>,
    },
    /// Compare the local installed version with the latest repository version
    Diff { name: String },
}
```

Subcommand behavior matrix:

| # | Subcommand | `--repo=false` (default) | `--repo=true` |
|---|------------|--------------------------|---------------|
| 1 | info | Query locally installed package info | Query latest repo package info |
| 2 | list-files | List files of the installed package | List files from repo metadata |
| 3 | owns-file | Find file owner among installed packages | Find file owner in repo metadata |
| 4 | deps | Dependencies of the installed package | Dependencies of the latest repo version |
| 5 | rdeps | Reverse deps of the installed package | Full repo reverse deps |
| 6 | search | Search installed packages | Search repo packages |
| 7 | list-installed | List all installed packages | N/A (local only) |
| 8 | list-repo | N/A (repo only) | List all available repo packages |
| 9 | metadata | Show local raw metadata | Show repo raw metadata |
| 10 | changelog | Local change log | Repo change log |
| 11 | source | Source info of the local package | Source info from the repo |
| 12 | generate-download-cmd | Auto-confirm online, then emit the download command | N/A |
| 13 | diff | Compare local version with the latest repo version | N/A |

`generate-download-cmd` output format:
- RPM family: `dnf download <pkg_name>`
- DEB family: `apt-get download <pkg_name>`

`generate-download-cmd` accepts two input styles:
- `pkq generate-download-cmd htop` — package name directly
- `pkq generate-download-cmd --file /usr/bin/htop` — a file path; the tool
  resolves the owning package first, then generates the command

`diff` output (Human):

```
Package: nginx
Local:   1.18.0-1ubuntu3
Repo:    1.24.0-1ubuntu3
Status:  OUTDATED
```

## 4. Key Business Flows (textual)

### 4.1 Flow: real-time online repository search (`search --repo`)

1. User runs `pkq search nginx --repo --refresh`.
2. The CLI builds `CacheConfig { force_refresh: true, ... }`.
3. The backend iterates the configured repo list and submits each repo URL to
   the `NetworkCacheManager`.
4. The manager connects to remote mirrors with the ureq HTTP client:
   - Checks the HTTP ETag; on 304 reuses the local cache directly.
   - If the index changed (200 OK), streams the downloaded data into
     `~/.cache/pkq/repos/`.
5. The adapter parses the fresh XML / control text and matches remote packages
   containing `nginx`.
6. The real package metadata list from the live repository is printed.

### 4.2 Flow: version comparison (`diff`)

1. User runs `pkq diff nginx --refresh`.
2. The CLI builds `CacheConfig { force_refresh: true, ... }` and calls
   `Backend.diff_versions("nginx", cfg)`.
3. The backend obtains nginx version info from the local source
   (rpmdb / dpkg status) and the online source (primary.xml / Packages.gz).
4. The `VersionDiff { local_version, repo_version, is_outdated }` result is
   formatted and printed.

## 5. Unified Data Model & Backend Trait (full definition)

### 5.1 Data model (src/model/mod.rs)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageSystem { Rpm, Deb }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageSource { Installed, Repo }

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub ttl_secs: u64,
    pub force_refresh: bool,
    pub offline_mode: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self { ttl_secs: 86400, force_refresh: false, offline_mode: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgMetadata {
    pub name: String,
    pub version: String,
    pub release: String,
    pub epoch: Option<String>,
    pub arch: String,
    pub summary: String,
    pub description: String,
    pub url: Option<String>,
    pub license: Option<String>,
    pub vendor: Option<String>,
    pub packager: Option<String>,
    pub source_pkg: Option<String>,
    pub size: Option<u64>,
    pub install_size: Option<u64>,
    pub group: Option<String>,
    pub build_time: Option<i64>,
    pub location: Option<String>,
    pub requires: Vec<Dependency>,
    pub provides: Vec<String>,
    pub conflicts: Vec<Dependency>,
    pub obsoletes: Vec<Dependency>,
    pub files: Vec<String>,
    pub changelog: Vec<ChangelogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub flags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub author: String,
    pub timestamp: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePackageInfo {
    pub name: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub url: Option<String>,
    pub license: Option<String>,
    pub binaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub name: String,
    pub local_version: Option<String>,
    pub repo_version: Option<String>,
    pub is_outdated: bool,
}
```

### 5.2 Backend Trait (src/backend/mod.rs)

```rust
pub trait PkgBackend: Send + Sync {
    fn system_type(&self) -> PackageSystem;

    fn get_package_details(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Option<PkgMetadata>>;
    fn list_files(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<String>>;
    fn find_file_owner(&self, file_path: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<String>>;
    fn get_dependencies(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<Dependency>>;
    fn get_reverse_dependencies(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<String>>;
    fn search_packages(&self, keyword: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<PkgMetadata>>;
    fn list_installed(&self) -> Result<Vec<PkgMetadata>>;
    fn list_repo_packages(&self, cfg: &CacheConfig) -> Result<Vec<PkgMetadata>>;
    fn get_raw_metadata(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<String>;
    fn get_changelog(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<ChangelogEntry>>;
    fn get_source_package(&self, name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Option<SourcePackageInfo>>;
    fn get_binaries_from_source(&self, source_name: &str, source: PackageSource, cfg: &CacheConfig) -> Result<Vec<String>>;
    fn generate_download_command(&self, name: &str, cfg: &CacheConfig) -> Result<String>;
    fn diff_versions(&self, name: &str, cfg: &CacheConfig) -> Result<VersionDiff>;
}
```

### 5.3 RPM Local Data Source Design

| Item | Description |
|------|-------------|
| Database path | `/var/lib/rpm/rpmdb.sqlite` (SQLite, RHEL 8+) or `/var/lib/rpm/Packages` (BDB, RHEL 7-) |
| Parsing | SQLite: read-only queries via `rusqlite`; BDB: degrade to repo-index-only mode (`--repo`) |
| File lists | Queried from rpmdb `Provide`/`Basenames` tables, equivalent to `rpm -ql` |
| Change log | Queried from the rpmdb `Changelog` table |

### 5.4 DEB Local Data Source Design

| Item | Description |
|------|-------------|
| Status file | `/var/lib/dpkg/status` (plain text, control-format paragraphs) |
| Parsing | Paragraph-wise control-format parsing extracting Package/Version/Architecture/Depends, etc. |
| File lists | Read from `/var/lib/dpkg/info/<pkg>.list` |
| Change log | Read from `/usr/share/doc/<pkg>/changelog.Debian.gz` (flate2 decompression) |

### 5.5 Output Format Design

| Format | Implementation |
|--------|----------------|
| Human | `colored` + `unicode-width` manual alignment (CJK width alignment for Chinese locales, bilingual tags) |
| JSON | `serde_json::to_string_pretty` on the corresponding struct |

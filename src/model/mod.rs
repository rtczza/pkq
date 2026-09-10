//! 领域模型：包元数据、依赖关系、搜索结果等核心数据结构。
//!
//! 所有结构体均实现 `Serialize`/`Deserialize`，是 JSON 输出（`--output json`）
//! 与缓存持久化（postcard）的统一载体。字段在 DEB/RPM 双后端间语义对齐。

use serde::{Deserialize, Serialize};

/// 包管理系统（发行版家族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageSystem {
    /// RPM 系（openEuler / Fedora / CentOS / UOS Server 等）。
    Rpm,
    /// DEB 系（Debian / Ubuntu / Deepin / UOS Desktop 等）。
    Deb,
}

/// 包数据来源：本地已安装或在线仓库。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageSource {
    /// 本地已安装（dpkg status / rpmdb）。
    Installed,
    /// 在线仓库元数据（apt / repodata）。
    Repo,
}

/// 缓存行为配置，由 CLI 全局选项（`--cache-ttl`/`--refresh`/`--offline`）构建。
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// 仓库元数据缓存有效期（秒），默认 86400（1 天）。
    pub ttl_secs: u64,
    /// 强制刷新缓存，忽略 TTL。
    pub force_refresh: bool,
    /// 离线模式：仅使用本地缓存，不发起网络请求。
    pub offline_mode: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl_secs: 86400,
            force_refresh: false,
            offline_mode: false,
        }
    }
}

/// 软件包元数据（跨后端统一视图）。
///
/// 字段按可用性填充：DEB 与 RPM 原生字段各有所长，无法获取的字段为 `None`/空。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PkgMetadata {
    /// 包名。
    pub name: String,
    /// 上游版本号。
    pub version: String,
    /// 发行版修订号（RPM release / DEB revision）。
    pub release: String,
    /// RPM epoch（DEB 一般为 `None`）。
    pub epoch: Option<String>,
    /// 目标架构（x86_64 / amd64 等，保留后端原生写法）。
    pub arch: String,
    /// 单行摘要。
    pub summary: String,
    /// 长描述。
    pub description: String,
    /// 项目主页。
    pub url: Option<String>,
    /// 许可证。
    pub license: Option<String>,
    /// 供应商。
    pub vendor: Option<String>,
    /// 打包者。
    pub packager: Option<String>,
    /// 源码包名。
    pub source_pkg: Option<String>,
    /// 包体积（字节）。
    pub size: Option<u64>,
    /// 安装后占用（字节）。
    pub install_size: Option<u64>,
    /// 分组（RPM group）。
    pub group: Option<String>,
    /// 优先级（DEB priority）。
    pub priority: Option<String>,
    /// 构建时间（Unix 时间戳）。
    pub build_time: Option<i64>,
    /// 仓库内相对位置（RPM location）。
    pub location: Option<String>,
    /// 元数据来源仓库标识。
    #[serde(default)]
    pub source_repo: Option<String>,
    /// 强依赖（DEB Depends / RPM Requires）。
    pub requires: Vec<Dependency>,
    /// 推荐依赖（DEB Recommends）。
    pub recommends: Vec<Dependency>,
    /// 建议依赖（DEB Suggests）。
    pub suggests: Vec<Dependency>,
    /// 提供能力（DEB Provides / RPM Provides，含 .so 能力）。
    pub provides: Vec<String>,
    /// 冲突关系。
    pub conflicts: Vec<Dependency>,
    /// 废弃关系（RPM Obsoletes）。
    pub obsoletes: Vec<Dependency>,
    /// 替代关系（DEB Replaces）。
    pub replaces: Vec<Dependency>,
    /// 包含的文件列表。
    pub files: Vec<String>,
    /// 变更日志条目。
    pub changelog: Vec<ChangelogEntry>,
}

/// 一条依赖关系：目标能力名 + 版本约束。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// 依赖目标（包名或能力名，如 `libc.so.6`）。
    pub name: String,
    /// 版本约束值。
    pub version: Option<String>,
    /// 版本比较符（RPM flags：GE/LT 等）。
    pub flags: Option<String>,
    /// 是否为可替代依赖（DEB `|` 备选）。
    #[serde(default)]
    pub is_alternative: bool,
}

/// 单条变更日志。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    /// 作者（维护者名与邮箱）。
    pub author: String,
    /// 时间戳（Unix 秒）。
    pub timestamp: i64,
    /// 变更内容。
    pub text: String,
}

/// 源码包下的二进制包信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryPackageInfo {
    /// 二进制包名。
    pub name: String,
    /// 版本。
    pub version: String,
    /// 架构。
    pub arch: String,
    /// 是否已本地安装。
    pub installed: bool,
    /// 元数据来源仓库标识。
    #[serde(default)]
    pub source_repo: Option<String>,
}

/// 源码包信息（`source` 子命令输出）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePackageInfo {
    /// 源码包名。
    pub name: String,
    /// 上游版本号。
    pub version: String,
    /// 发行版修订号。
    pub release: String,
    /// 项目主页。
    pub url: Option<String>,
    /// 许可证。
    pub license: Option<String>,
    /// 维护者。
    pub maintainer: Option<String>,
    /// 由该源码包产出的二进制包。
    pub binaries: Vec<BinaryPackageInfo>,
}

/// 单条搜索结果（`search` 子命令输出）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    /// 命中的包名。
    pub pkg_name: String,
    /// 命中文本（摘要片段或文件路径）。
    pub matched_text: String,
    /// 命中类型（keyword / path 等）。
    pub match_type: String,
    /// 数据来源（installed / repo）。
    pub source: String,
}

/// 单条反向依赖结果（`rdeps` 子命令输出）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseDep {
    /// 依赖方包名。
    pub pkg_name: String,
    /// 依赖方版本。
    pub version: String,
    /// 依赖方架构。
    #[serde(default)]
    pub arch: Option<String>,
    /// 命中的依赖条目（能力名 + 约束）。
    pub matched_deps: Vec<String>,
    /// 元数据来源仓库标识。
    #[serde(default)]
    pub source_repo: Option<String>,
}

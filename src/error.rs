//! 统一错误类型：全 crate 的错误收敛到 [`PkgError`]，经 `Result` 别名向上传递。

use thiserror::Error;

/// pkq 统一错误枚举。
///
/// `Display` 实现由 thiserror 生成，直接面向用户输出；
/// 常见外部错误（io/rusqlite/quick-xml/serde_json）已提供 `From` 转换。
#[derive(Error, Debug)]
pub enum PkgError {
    /// 网络请求失败（超时、状态码异常等）。
    #[error("Network error: {0}")]
    NetworkError(String),

    /// 文件系统 IO 失败。
    #[error("IO error: {0}")]
    IoError(String),

    /// 本地数据库（rpmdb sqlite）访问失败。
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// 数据解析失败（索引文件格式异常）。
    #[error("Parse error: {0}")]
    ParseError(String),

    /// 目标软件包未找到。
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    /// 仓库配置或元数据异常。
    #[error("Repository error: {0}")]
    RepoError(String),

    /// 解压缩失败（gz/lz4 等）。
    #[error("Compression error: {0}")]
    CompressionError(String),

    /// XML 解析失败（repodata）。
    #[error("XML error: {0}")]
    XmlError(String),

    /// 非法参数（用户输入或内部约定违背）。
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

impl From<std::io::Error> for PkgError {
    fn from(e: std::io::Error) -> Self {
        PkgError::IoError(e.to_string())
    }
}

impl From<rusqlite::Error> for PkgError {
    fn from(e: rusqlite::Error) -> Self {
        PkgError::DatabaseError(e.to_string())
    }
}

impl From<quick_xml::Error> for PkgError {
    fn from(e: quick_xml::Error) -> Self {
        PkgError::XmlError(e.to_string())
    }
}

impl From<serde_json::Error> for PkgError {
    fn from(e: serde_json::Error) -> Self {
        PkgError::ParseError(format!("JSON: {}", e))
    }
}

/// crate 级 `Result` 别名。
pub type Result<T> = std::result::Result<T, PkgError>;

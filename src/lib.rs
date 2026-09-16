//! pkq — 跨发行版（DEB/RPM）Linux 软件包查询库
//!
//! 对外暴露的核心抽象：
//! - [`backend::PkgBackend`]：包管理后端 trait（DebBackend / RpmBackend）
//! - [`engine::run`]：命令执行入口，返回 [`engine::ExitStatus`]
//! - [`model`]：领域模型（PkgMetadata / CacheConfig / SearchResult 等）
//! - [`error::PkgError`]：统一错误类型
//!
//! 二进制入口在 `main.rs`，仅负责 CLI 解析、SIGPIPE 恢复与退出码映射。

pub mod backend;
pub mod cache;
pub mod cli;
pub mod completion;
pub mod engine;
pub mod error;
pub mod i18n;
pub mod logging;
pub mod model;
pub mod network;
pub mod output;
pub mod path_util;

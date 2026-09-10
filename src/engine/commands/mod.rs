//! 各子命令的业务编排。
//!
//! 每个模块一个 `run` 函数，通过 [`super::Ctx`] 访问后端/配置/输出格式，
//! 返回 [`super::ExitStatus`]；除字段解析与编排外不含格式化细节（在 output.rs）。

pub mod cache;
pub mod changelog;
pub mod deps;
pub mod info;
pub mod list;
pub mod owns;
pub mod rdeps;
pub mod search;
pub mod source;

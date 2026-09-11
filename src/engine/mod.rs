//! 命令执行引擎：子命令分发、共享上下文与退出码语义。
//!
//! 各子命令的业务编排位于 `commands` 子模块；横切辅助（Banner、
//! 时间格式化、词边界匹配、文件查询等）位于 `support`。

mod commands;
pub(crate) mod support;

use crate::backend::PkgBackend;
use crate::cli::{Commands, OutputFormat};
use crate::error::Result;
use crate::model::CacheConfig;

/// 统一退出码语义：Hit=0（查询命中），NotFound=1（无结果）。
/// 硬错误经 [`Result::Err`] 传递，由 main 以退出码 2 上报（可用 `--legacy-exit-code` 回退旧行为）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    Hit,
    NotFound,
}

/// 命令执行上下文：一次 run 内共享后端、配置、输出格式与 Banner 打印状态。
pub(crate) struct Ctx<'a> {
    pub backend: &'a dyn PkgBackend,
    pub cfg: &'a CacheConfig,
    pub fmt: OutputFormat,
    /// 保证每条命令最多打印一次元数据 Banner
    pub banner_shown: &'a mut bool,
}

pub fn run(command: &Commands, cfg: &CacheConfig, fmt: OutputFormat) -> Result<ExitStatus> {
    let backend = support::detect_backend()?;
    let mut banner_shown = false;
    let mut ctx = Ctx {
        backend: backend.as_ref(),
        cfg,
        fmt,
        banner_shown: &mut banner_shown,
    };
    match command {
        Commands::Info { name, repo } => commands::info::run(&mut ctx, name, *repo),
        Commands::List { name, repo, all } => commands::list::run(&mut ctx, name, *repo, *all),
        Commands::Owns { path, repo, all } => commands::owns::run(&mut ctx, path, *repo, *all),
        Commands::Deps { name, repo } => commands::deps::run(&mut ctx, name, *repo),
        Commands::RDeps {
            name,
            repo,
            installed_only,
            all,
        } => commands::rdeps::run(&mut ctx, name, *repo, *installed_only, *all),
        Commands::Search {
            pattern,
            regex,
            names_only,
            files_only,
            all_files,
            max_files,
            installed,
            repo,
        } => commands::search::run(
            &mut ctx,
            commands::search::SearchArgs {
                pattern,
                regex: *regex,
                names_only: *names_only,
                files_only: *files_only,
                all_files: *all_files,
                max_files: *max_files,
                installed: *installed,
                repo: *repo,
            },
        ),
        Commands::Source { name, repo: _ } => commands::source::run(&mut ctx, name),
        Commands::Changelog { name, repo } => commands::changelog::run(&mut ctx, name, *repo),
        Commands::Cache { cmd } => commands::cache::run(&mut ctx, cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::support::*;
    use crate::model::PackageSystem;

    #[test]
    fn test_is_common_dir() {
        assert!(is_common_dir("/"));
        assert!(is_common_dir("/usr/bin"));
        assert!(is_common_dir("/usr/bin/"));
        assert!(!is_common_dir("/usr/bin/custom"));
    }

    #[test]
    fn test_filter_common_dirs() {
        let files = vec![
            "/usr/bin/foo".to_string(),
            "/etc".to_string(),
            "/opt/app/x".to_string(),
        ];
        let filtered = filter_common_dirs(&files);
        assert_eq!(
            filtered,
            vec!["/usr/bin/foo".to_string(), "/opt/app/x".to_string()]
        );
    }

    #[test]
    fn test_is_path_query() {
        assert!(is_path_query("/bin/ls"));
        assert!(is_path_query("*/bin/unzip"));
        assert!(!is_path_query("vim"));
    }

    #[test]
    fn test_system_from_id_tokens() {
        assert_eq!(
            system_from_id_tokens("deepin", "debian"),
            Some(PackageSystem::Deb)
        );
        assert_eq!(
            system_from_id_tokens("fedora", ""),
            Some(PackageSystem::Rpm)
        );
        assert_eq!(
            system_from_id_tokens("centos", "rhel fedora"),
            Some(PackageSystem::Rpm)
        );
        assert_eq!(system_from_id_tokens("uos", ""), None); // UOS 歧义 → 路径兜底
        assert_eq!(
            system_from_id_tokens("ubuntu", "debian"),
            Some(PackageSystem::Deb)
        );
        assert_eq!(
            system_from_id_tokens("openeuler", ""),
            Some(PackageSystem::Rpm)
        );
    }
}

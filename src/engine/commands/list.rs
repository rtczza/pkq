//! `list` 命令：文件列表（本地优先；未安装自动转仓库检索并给出安装引导）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::model::PackageSource;
use crate::{i18n::Language, output};

pub(crate) fn run(ctx: &mut Ctx, name: &str, repo: bool, all: bool) -> Result<ExitStatus> {
    let l = Language::current();
    if !repo {
        // 1. 本地优先查询
        let local_pkg = ctx
            .backend
            .get_package_details(name, PackageSource::Installed, ctx.cfg)?;
        if local_pkg.is_some() {
            let mut files = ctx
                .backend
                .list_files(name, PackageSource::Installed, ctx.cfg)?;
            // 按路径排序：dpkg/rpm 数据库的原始顺序由打包时归档顺序决定
            // （同目录文件可能被 alternatives 等机制拆到尾部），排序后同目录聚合
            files.sort();
            let filtered = if all {
                files
            } else {
                support::filter_common_dirs(&files)
            };
            if filtered.is_empty() {
                eprintln!("{}", l.no_specific_files(name));
            } else {
                output::print_file_list(&filtered, ctx.fmt)?;
            }
            return Ok(ExitStatus::Hit);
        }

        // 2. 本地未安装：必须打印提示并自动转入仓库检索引导（与 DEB 严格对齐）
        eprintln!("{}", l.not_installed_searching(name));
        support::show_banner(ctx.banner_shown);
        match ctx
            .backend
            .get_package_details(name, PackageSource::Repo, ctx.cfg)?
        {
            Some(_) => {
                eprintln!("{}", l.found_in_repo(name));
                eprintln!(
                    "{}: {}",
                    l.install_cmd_label(),
                    support::install_cmd(ctx.backend, name)
                );
                Ok(ExitStatus::Hit)
            }
            None => {
                eprintln!("{}", l.not_found_both(name));
                Ok(ExitStatus::NotFound)
            }
        }
    } else {
        // 显式指定了 --repo
        support::show_banner(ctx.banner_shown);
        match ctx
            .backend
            .get_package_details(name, PackageSource::Repo, ctx.cfg)?
        {
            Some(_) => {
                let mut files = ctx.backend.list_files(name, PackageSource::Repo, ctx.cfg)?;
                files.sort();
                let filtered = if all {
                    files
                } else {
                    support::filter_common_dirs(&files)
                };
                if filtered.is_empty() {
                    eprintln!("{}", l.found_in_repo(name));
                    eprintln!(
                        "{}: {}",
                        l.install_cmd_label(),
                        support::install_cmd(ctx.backend, name)
                    );
                } else {
                    // P0-9：仓库文件列表须标明来源
                    eprintln!("{}", l.repo_file_list_header(name));
                    output::print_file_list(&filtered, ctx.fmt)?;
                }
                Ok(ExitStatus::Hit)
            }
            None => {
                eprintln!("{}", l.not_found_repo(name));
                Ok(ExitStatus::NotFound)
            }
        }
    }
}

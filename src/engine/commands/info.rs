//! `info` 命令：软件包详情（本地优先，未安装自动降级仓库并给出安装引导）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::model::PackageSource;
use crate::{i18n::Language, output};

pub(crate) fn run(ctx: &mut Ctx, name: &str, repo: bool) -> Result<ExitStatus> {
    let l = Language::current();
    if repo {
        support::show_banner(ctx.banner_shown);
        match ctx
            .backend
            .get_package_details(name, PackageSource::Repo, ctx.cfg)?
        {
            Some(mut pkg) => {
                support::supplement_source_pkg(&mut pkg);
                output::print_package_details(Some(pkg), ctx.fmt)?;
                Ok(ExitStatus::Hit)
            }
            None => {
                eprintln!("{}", l.not_found_repo(name));
                Ok(ExitStatus::NotFound)
            }
        }
    } else {
        match ctx
            .backend
            .get_package_details(name, PackageSource::Installed, ctx.cfg)?
        {
            Some(mut pkg) => {
                support::supplement_source_pkg(&mut pkg);
                output::print_package_details(Some(pkg), ctx.fmt)?;
                Ok(ExitStatus::Hit)
            }
            None => {
                support::show_banner(ctx.banner_shown);
                eprintln!("{}", l.not_installed_searching(name));
                match ctx
                    .backend
                    .get_package_details(name, PackageSource::Repo, ctx.cfg)?
                {
                    Some(mut repo_pkg) => {
                        support::supplement_source_pkg(&mut repo_pkg);
                        output::print_package_details(Some(repo_pkg), ctx.fmt)?;
                        eprintln!(
                            "\n{}: {}",
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
            }
        }
    }
}

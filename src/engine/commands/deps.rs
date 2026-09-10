//! `deps` 命令：依赖关系（RPM 端做 capability→真实包名反查后分段输出）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::model::{PackageSource, PackageSystem};
use crate::{i18n::Language, output};

pub(crate) fn run(ctx: &mut Ctx, name: &str, repo: bool) -> Result<ExitStatus> {
    let l = Language::current();
    let source = if repo {
        PackageSource::Repo
    } else {
        PackageSource::Installed
    };
    if repo {
        support::show_banner(ctx.banner_shown);
    }
    let pkg_opt = ctx.backend.get_package_details(name, source, ctx.cfg)?;
    if let Some(pkg) = pkg_opt {
        let has_any = !pkg.requires.is_empty()
            || !pkg.recommends.is_empty()
            || !pkg.suggests.is_empty()
            || !pkg.conflicts.is_empty()
            || !pkg.obsoletes.is_empty()
            || !pkg.replaces.is_empty();
        if !has_any {
            eprintln!("{}", l.no_deps(name));
        } else if ctx.backend.system_type() == PackageSystem::Rpm {
            output::print_full_dependencies_resolved(&pkg, ctx.fmt, |dep_name| {
                ctx.backend.resolve_dep_name(dep_name)
            })?;
        } else {
            output::print_full_dependencies(&pkg, ctx.fmt)?;
        }
        Ok(ExitStatus::Hit)
    } else if repo {
        eprintln!("{}", l.not_found_repo(name));
        Ok(ExitStatus::NotFound)
    } else {
        eprintln!("{}", l.not_installed_searching(name));
        support::show_banner(ctx.banner_shown);
        match ctx
            .backend
            .get_package_details(name, PackageSource::Repo, ctx.cfg)?
        {
            Some(repo_pkg) => {
                // 仓库 fallback 同样做能力→真实包名反查（与主路径一致）
                if ctx.backend.system_type() == PackageSystem::Rpm {
                    output::print_full_dependencies_resolved(&repo_pkg, ctx.fmt, |dep_name| {
                        ctx.backend.resolve_dep_name(dep_name)
                    })?;
                } else {
                    output::print_full_dependencies(&repo_pkg, ctx.fmt)?;
                }
                Ok(ExitStatus::Hit)
            }
            None => {
                eprintln!("{}", l.not_found_both(name));
                Ok(ExitStatus::NotFound)
            }
        }
    }
}

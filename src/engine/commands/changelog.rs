//! `changelog` 命令：变更日志（本地优先，未命中自动降级仓库）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::i18n::Language;
use crate::model::PackageSource;
use crate::output;

pub(crate) fn run(ctx: &mut Ctx, name: &str, repo: bool) -> Result<ExitStatus> {
    let l = Language::current();
    let source = if repo {
        PackageSource::Repo
    } else {
        PackageSource::Installed
    };
    match ctx.backend.get_changelog(name, source, ctx.cfg) {
        Ok(cl) if !cl.is_empty() => {
            output::print_changelog(&cl, ctx.fmt)?;
            Ok(ExitStatus::Hit)
        }
        _ if !repo => {
            eprintln!("{}", l.searching_repo_changelog());
            support::show_banner(ctx.banner_shown);
            match ctx
                .backend
                .get_changelog(name, PackageSource::Repo, ctx.cfg)
            {
                Ok(cl) if !cl.is_empty() => {
                    output::print_changelog(&cl, ctx.fmt)?;
                    Ok(ExitStatus::Hit)
                }
                _ => {
                    eprintln!("{}", l.no_changelog(name));
                    Ok(ExitStatus::NotFound)
                }
            }
        }
        _ => {
            eprintln!("{}", l.no_changelog_repo());
            Ok(ExitStatus::NotFound)
        }
    }
}

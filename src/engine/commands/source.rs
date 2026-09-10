//! `source` 命令：源码包与二进制包互查（双端统一基于仓库全量元数据）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::model::PackageSource;
use crate::{i18n::Language, output};

pub(crate) fn run(ctx: &mut Ctx, name: &str) -> Result<ExitStatus> {
    let l = Language::current();
    support::show_banner(ctx.banner_shown);
    match ctx
        .backend
        .get_source_package(name, PackageSource::Repo, ctx.cfg)?
    {
        Some(info) => {
            output::print_source_package(Some(info), ctx.fmt)?;
            Ok(ExitStatus::Hit)
        }
        None => {
            eprintln!("{}", l.source_not_found(name));
            Ok(ExitStatus::NotFound)
        }
    }
}

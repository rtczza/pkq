//! `owns` 命令：文件归属查询（支持通配符；目录共享限流展示）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::model::PackageSource;

pub(crate) fn run(ctx: &mut Ctx, path: &str, repo: bool, all: bool) -> Result<ExitStatus> {
    if repo {
        support::show_banner(ctx.banner_shown);
    }
    let source = if repo {
        PackageSource::Repo
    } else {
        PackageSource::Installed
    };
    support::handle_file_query(ctx, path, source, all)
}

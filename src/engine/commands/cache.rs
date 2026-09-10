//! `cache` 子命令：status / update / clean（P1-4）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;

pub(crate) fn run(ctx: &mut Ctx, cmd: &crate::cli::CacheCmd) -> Result<ExitStatus> {
    support::handle_cache(ctx, cmd)
}

//! `rdeps` 命令：反向依赖（本地 + 仓库合并去重，状态标签与底部统计严格一致）。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::i18n::Language;
use crate::model::{PackageSource, ReverseDep};
use crate::output;

pub(crate) fn run(
    ctx: &mut Ctx,
    name: &str,
    repo: bool,
    installed_only: bool,
    all: bool,
) -> Result<ExitStatus> {
    let l = Language::current();
    // --installed-only 为纯本地查询，不触发仓库元数据
    if !installed_only {
        support::show_banner(ctx.banner_shown);
    }
    let local_rdeps = if repo {
        Vec::new()
    } else {
        ctx.backend
            .get_reverse_dependencies(name, PackageSource::Installed, ctx.cfg)
            .unwrap_or_default()
    };
    let repo_rdeps = if installed_only {
        Vec::new()
    } else {
        ctx.backend
            .get_reverse_dependencies(name, PackageSource::Repo, ctx.cfg)
            .unwrap_or_default()
    };
    let mut merged: Vec<ReverseDep> = local_rdeps;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in &merged {
        seen.insert(r.pkg_name.clone());
    }
    for r in repo_rdeps {
        if seen.insert(r.pkg_name.clone()) {
            merged.push(r);
        }
    }
    if merged.is_empty() {
        eprintln!("{}", l.no_rdeps(name));
        return Ok(ExitStatus::NotFound);
    }
    let installed_count = merged.iter().filter(|r| r.source_repo.is_none()).count();
    let repo_count = merged.len() - installed_count;
    // 默认限流 50 防刷屏；--all 查看全部（需要前 N 条请配合 | head -N）
    let default_limit = crate::backend::common::DEFAULT_PAGE_LIMIT;
    let truncated = !all && merged.len() > default_limit;
    let display = if truncated {
        &merged[..default_limit]
    } else {
        &merged[..]
    };
    output::print_reverse_dependencies(display, ctx.fmt)?;
    let summary = l.rdeps_summary(merged.len(), installed_count, repo_count);
    let extra = if truncated {
        format!(" {}", l.limit_note(default_limit))
    } else {
        String::new()
    };
    println!("\n{}{}", summary, extra);
    Ok(ExitStatus::Hit)
}

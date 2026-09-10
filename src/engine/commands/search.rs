//! `search` 命令：关键词 + 文件路径双索引检索，词边界相关性分层，
//! 模糊命中折叠为「可能相关」段。

use crate::engine::{support, Ctx, ExitStatus};
use crate::error::Result;
use crate::i18n::Language;
use crate::model::{PackageSource, SearchResult};
use crate::output;

/// `search` 子命令的解析后参数（收敛 8 个位置参数，消除 clippy 豁免）。
pub(crate) struct SearchArgs<'a> {
    pub pattern: &'a str,
    pub regex: bool,
    pub names_only: bool,
    pub files_only: bool,
    pub all_files: bool,
    pub max_files: usize,
    pub installed: bool,
    pub repo: bool,
}

pub(crate) fn run(ctx: &mut Ctx, args: SearchArgs<'_>) -> Result<ExitStatus> {
    let SearchArgs {
        pattern,
        regex,
        names_only,
        files_only,
        all_files,
        max_files,
        installed,
        repo,
    } = args;
    let l = Language::current();
    if support::is_path_query(pattern) {
        let source = if repo {
            PackageSource::Repo
        } else {
            PackageSource::Installed
        };
        if repo {
            support::show_banner(ctx.banner_shown);
        }
        let st = support::handle_file_query(ctx, pattern, source, all_files)?;
        // apt-file 语义：本地无归属且未显式限定已安装时，回退仓库文件索引检索
        if st == ExitStatus::NotFound && !repo && !installed {
            eprintln!("{}", l.searching_repo_files());
            support::show_banner(ctx.banner_shown);
            return support::handle_file_query(ctx, pattern, PackageSource::Repo, all_files);
        }
        return Ok(st);
    }
    // 非路径查询：--installed-only 为纯本地检索，不触发仓库元数据
    if !installed {
        support::show_banner(ctx.banner_shown);
    }
    let file_limit = if all_files { 0 } else { max_files };
    let mut all_keywords: Vec<SearchResult> = Vec::new();
    let mut related_keywords: Vec<SearchResult> = Vec::new();
    let mut all_files_vec: Vec<SearchResult> = Vec::new();

    let (local_results, repo_results) = if names_only {
        let local = support::keyword_search(
            ctx.backend,
            pattern,
            regex,
            PackageSource::Installed,
            ctx.cfg,
        )?;
        let repo = if installed {
            Vec::new()
        } else {
            support::keyword_search(ctx.backend, pattern, regex, PackageSource::Repo, ctx.cfg)?
        };
        (local, repo)
    } else {
        let local =
            ctx.backend
                .search_by_pattern(pattern, regex, PackageSource::Installed, ctx.cfg)?;
        let repo = if installed {
            Vec::new()
        } else {
            ctx.backend
                .search_by_pattern(pattern, regex, PackageSource::Repo, ctx.cfg)?
        };
        (local, repo)
    };

    if !files_only {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in local_results.iter().chain(repo_results.iter()) {
            if r.match_type == "keyword" && seen.insert(r.pkg_name.clone()) {
                all_keywords.push(r.clone());
            }
        }
        let pattern_lower = pattern.to_lowercase();
        // 相关性分层（词边界优先）：
        //   0/1/2 包名词边界命中（精确/前缀/包含） > 3 摘要词边界命中
        //   > 4 包名非边界命中（如 sudo→sudoku） > 5 摘要子串命中
        let name_rank = |r: &SearchResult| -> u8 {
            let name = r.pkg_name.to_lowercase();
            if name == pattern_lower {
                0
            } else if name.starts_with(&pattern_lower) {
                1
            } else if name.contains(&pattern_lower) {
                2
            } else {
                3
            }
        };
        let tier = |r: &SearchResult| -> u8 {
            let name = r.pkg_name.to_lowercase();
            if name.contains(&pattern_lower) {
                if support::word_boundary_contains(&name, &pattern_lower) {
                    name_rank(r)
                } else {
                    4
                }
            } else if support::word_boundary_contains(
                &r.matched_text.to_lowercase(),
                &pattern_lower,
            ) {
                3
            } else {
                5
            }
        };
        all_keywords.sort_by(|a, b| {
            let (ta, a_len) = (tier(a), a.pkg_name.len());
            let (tb, b_len) = (tier(b), b.pkg_name.len());
            ta.cmp(&tb)
                .then_with(|| a_len.cmp(&b_len))
                .then_with(|| {
                    let a_inst = a.source == "installed";
                    let b_inst = b.source == "installed";
                    b_inst.cmp(&a_inst)
                })
                .then_with(|| a.pkg_name.cmp(&b.pkg_name))
        });
        // 主列表仅保留【包名命中】（tier 0/1/2/4），仅摘要命中
        // （词边界 tier3 与模糊 tier5）全部折叠为"可能相关"段，--all 展开。
        // 与 dnf search 的分层语义对齐（名称精确 > 名称/摘要 > 摘要），
        // 显著提升主列表信噪比（bash 场景过滤掉 bashate/bashful 等噪音包）。
        related_keywords = all_keywords
            .iter()
            .filter(|r| tier(r) == 3 || tier(r) == 5)
            .cloned()
            .collect();
        all_keywords.retain(|r| tier(r) != 3 && tier(r) != 5);
    }
    if !names_only {
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for r in local_results.iter().chain(repo_results.iter()) {
            if r.match_type == "file" && seen.insert((r.pkg_name.clone(), r.matched_text.clone())) {
                all_files_vec.push(r.clone());
            }
        }
    }

    if all_keywords.is_empty() && related_keywords.is_empty() && all_files_vec.is_empty() {
        eprintln!("{}", l.no_matches());
        return Ok(ExitStatus::NotFound);
    }
    output::print_search_partitioned(
        &all_keywords,
        &related_keywords,
        &all_files_vec,
        file_limit,
        all_files,
        ctx.fmt,
    )?;
    Ok(ExitStatus::Hit)
}

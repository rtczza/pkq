//! Tab 补全：基于 clap_complete 动态补全（CompleteEnv）的候选器。
//!
//! 注册方式：`source <(COMPLETE=bash pkq)`（bash）/ `source <(COMPLETE=zsh pkq)`（zsh），
//! 安装器已自动写入 rc 文件。候选生成由 shell 包装函数回调本二进制完成，
//! 因此补全逻辑必须轻量：仅读本地数据库，不触网、不打印任何输出。

use std::collections::BTreeMap;
use std::ffi::OsStr;

use clap_complete::engine::{CompletionCandidate, PathCompleter, ValueCompleter as _};

use crate::model::PackageSystem;

/// 单条候选摘要的最大显示宽度（超出截断，避免补全菜单换行刷屏）
const HELP_LIMIT: usize = 72;

/// 候选过滤核心（纯函数）：包名按前缀过滤（大小写不敏感），
/// 同名去重（dpkg 多架构同名段落），按名称排序，摘要作为候选说明。
pub(crate) fn package_candidates_from(
    prefix: &str,
    packages: impl Iterator<Item = (String, String)>,
) -> Vec<CompletionCandidate> {
    let prefix = prefix.to_lowercase();
    let mut matched: BTreeMap<String, String> = BTreeMap::new();
    for (name, summary) in packages {
        if !name.to_lowercase().starts_with(&prefix) {
            continue;
        }
        matched.entry(name).or_insert(summary);
    }
    matched
        .into_iter()
        .map(|(name, summary)| -> CompletionCandidate {
            let mut candidate = CompletionCandidate::new(name);
            if !summary.is_empty() {
                let help = if summary.chars().count() > HELP_LIMIT {
                    format!("{}…", summary.chars().take(HELP_LIMIT).collect::<String>())
                } else {
                    summary
                };
                candidate = candidate.help(Some(help.into()));
            }
            candidate
        })
        .collect()
}

/// 本地已安装包名候选（info/list/deps/rdeps/source/changelog 的 name 位置参数）
pub(crate) fn package_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    package_candidates_from(&current.to_string_lossy(), installed_packages())
}

/// search 的 pattern 位置参数：以 / 开头按路径补全，否则按包名补全
/// （与 search 自身“路径查询转 owns”的语义一致）
pub(crate) fn search_pattern_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    if current.to_string_lossy().starts_with('/') {
        return PathCompleter::any().complete(current);
    }
    package_completer(current)
}

/// cache clean 的 target 位置参数
pub(crate) fn cache_target_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    ["all", "index", "repos", "contents"]
        .into_iter()
        .filter(|t| t.starts_with(current.to_string_lossy().as_ref()))
        .map(CompletionCandidate::new)
        .collect()
}

/// 已安装包名 + 摘要，带补全缓存（失效键 = 本地数据库 mtime）。
/// TAB 补全每次按键都会拉起本进程，必须避免全量解析 dpkg status / rpmdb
/// （实测解析占补全延迟 96%）；本地数据库不可用时返回空（补全降级为无候选）。
fn installed_packages() -> impl Iterator<Item = (String, String)> {
    type BuildFn = fn() -> Vec<(String, String)>;
    let (source, build): (Option<std::path::PathBuf>, BuildFn) =
        match crate::engine::support::detect_system() {
            Some(PackageSystem::Deb) => (
                Some(std::path::PathBuf::from("/var/lib/dpkg/status")),
                build_deb_names,
            ),
            Some(PackageSystem::Rpm) => {
                (crate::backend::rpm::local::rpm_db_path(), build_rpm_names)
            }
            None => (None, Vec::new as BuildFn),
        };
    let Some(source) = source else {
        return Vec::new().into_iter();
    };
    let source_mtime = std::fs::metadata(&source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache_path = crate::cache::completion_names_cache_path();
    if let Some(cached) = crate::cache::CompletionNamesCache::load(&cache_path, source_mtime) {
        return cached.packages.into_iter();
    }
    let packages = build();
    if !packages.is_empty() {
        let _ =
            crate::cache::CompletionNamesCache::save(&cache_path, source_mtime, packages.clone());
    }
    packages.into_iter()
}

fn build_deb_names() -> Vec<(String, String)> {
    crate::backend::deb::local::DebLocal::load()
        .map(|l| {
            l.packages
                .iter()
                .map(|p| (p.name.clone(), p.summary.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn build_rpm_names() -> Vec<(String, String)> {
    crate::backend::rpm::local::RpmLocal::load()
        .map(|l| {
            l.packages
                .iter()
                .map(|p| (p.name.clone(), p.summary.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(cands: &[CompletionCandidate]) -> Vec<String> {
        cands
            .iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn test_package_candidates_filter_dedup_sort() {
        let pkgs = [
            ("libgnutls-dane0".to_string(), "DANE library".to_string()),
            ("LibGnuTLS30".to_string(), "TLS".to_string()),
            ("libgnutls-dane0".to_string(), "dup".to_string()),
            ("bash".to_string(), String::new()),
        ];
        let cands = package_candidates_from("libgnutls", pkgs.iter().cloned());
        // 大小写不敏感前缀过滤；同名段落去重（保留首个摘要）；按名称字节序排序
        assert_eq!(names(&cands), ["LibGnuTLS30", "libgnutls-dane0"]);
        assert_eq!(
            cands[0].get_help().map(|h| h.to_string()),
            Some("TLS".into())
        );
        // 空摘要不设置 help
        assert_eq!(
            package_candidates_from("bash", pkgs.iter().cloned())[0].get_help(),
            None
        );
    }

    #[test]
    fn test_summary_truncated() {
        let long = "x".repeat(100);
        let cands = package_candidates_from("", [("a".to_string(), long)].into_iter());
        let help = cands[0].get_help().unwrap().to_string();
        assert_eq!(help.chars().count(), HELP_LIMIT + 1);
        assert!(help.ends_with('…'));
    }

    #[test]
    fn test_cache_target_completer() {
        assert_eq!(names(&cache_target_completer(OsStr::new("in"))), ["index"]);
        assert_eq!(names(&cache_target_completer(OsStr::new(""))).len(), 4);
        assert!(cache_target_completer(OsStr::new("xyz")).is_empty());
    }
}

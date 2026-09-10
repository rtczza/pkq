use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Zh,
    En,
}

static LANG: OnceLock<Language> = OnceLock::new();

impl Language {
    pub fn detect() -> Self {
        let lang = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();

        if lang.to_lowercase().starts_with("zh") {
            Language::Zh
        } else {
            Language::En
        }
    }

    pub fn current() -> Self {
        *LANG.get_or_init(Language::detect)
    }

    pub fn installed_tag(self) -> &'static str {
        match self {
            Language::Zh => "[已安装]",
            Language::En => "[installed]",
        }
    }

    pub fn repo_tag(self) -> &'static str {
        match self {
            Language::Zh => "[未安装]",
            Language::En => "[repo]",
        }
    }

    /// 仓库侧结果标注（P0-9 来源标注体系）
    pub fn repo_pkg_tag(self) -> &'static str {
        match self {
            Language::Zh => "[仓库]",
            Language::En => "[repo]",
        }
    }

    pub fn searching_repo_files(self) -> String {
        match self {
            Language::Zh => "本地未找到归属，正在检索仓库文件索引…".to_string(),
            Language::En => {
                "Not owned by any installed package, searching repository file index...".to_string()
            }
        }
    }

    /// 仓库文件列表来源标头（P0-9）
    pub fn repo_file_list_header(self, pkg: &str) -> String {
        match self {
            Language::Zh => format!("本地未安装 {}，以下为仓库版本的文件列表：", pkg),
            Language::En => format!(
                "{} is not installed locally, file list below is from the repository:",
                pkg
            ),
        }
    }

    // cache 子命令（P1-4）
    pub fn cache_dir_label(self) -> &'static str {
        match self {
            Language::Zh => "缓存目录:",
            Language::En => "Cache dir:",
        }
    }

    pub fn cache_total_label(self) -> &'static str {
        match self {
            Language::Zh => "总计",
            Language::En => "Total",
        }
    }

    pub fn cache_clean_confirm(self, target: &str, bytes: u64) -> String {
        match self {
            Language::Zh => format!(
                "即将清理缓存目标 '{}'（约 {}）。这是安全操作（仅删除缓存，查询时会自动重建）。确认请追加 --yes。",
                target,
                crate::output::format_size_public(bytes)
            ),
            Language::En => format!(
                "About to clean cache target '{}' (~{}). This is safe (cache only, rebuilt on demand). Append --yes to proceed.",
                target,
                crate::output::format_size_public(bytes)
            ),
        }
    }

    pub fn cache_cleaned(self, bytes: u64) -> String {
        match self {
            Language::Zh => format!(
                "已释放 {} 缓存空间。",
                crate::output::format_size_public(bytes)
            ),
            Language::En => format!(
                "Freed {} of cache.",
                crate::output::format_size_public(bytes)
            ),
        }
    }

    pub fn packages_header(self, count: usize) -> String {
        match self {
            Language::Zh => format!("==> 软件包 (共 {} 个)", count),
            Language::En => format!("==> Packages ({} packages)", count),
        }
    }

    pub fn files_header(self, count: usize) -> String {
        match self {
            Language::Zh => format!("==> 文件路径匹配 (来自 {} 个软件包)", count),
            Language::En => format!("==> Files (in {} packages)", count),
        }
    }

    pub fn more_files(self, count: usize) -> String {
        match self {
            Language::Zh => format!("... (还有 {} 个文件，可使用 --all 查看全部)", count),
            Language::En => format!("... ({} more, use --all to view all)", count),
        }
    }

    pub fn related_header(self, count: usize) -> String {
        match self {
            Language::Zh => format!("==> 可能相关 (共 {} 个)", count),
            Language::En => format!("==> Related ({} packages)", count),
        }
    }

    pub fn rdeps_summary(self, total: usize, inst: usize, repo: usize) -> String {
        match self {
            Language::Zh => format!(
                "共 {} 个反向依赖包（已安装: {}, 仓库: {}）",
                total, inst, repo
            ),
            Language::En => format!(
                "Total {} reverse dependencies (installed: {}, repo: {})",
                total, inst, repo
            ),
        }
    }

    pub fn limit_note(self, limit: usize) -> String {
        match self {
            Language::Zh => format!("（仅显示前 {} 个，使用 --all 查看全部）", limit),
            Language::En => format!("(showing first {}, use --all to show all)", limit),
        }
    }

    pub fn metadata_refreshed(self, count: usize, secs: f64) -> String {
        match self {
            Language::Zh => format!(
                "已刷新仓库元数据：共 {} 个软件包，耗时 {:.1} 秒。",
                count, secs
            ),
            Language::En => format!(
                "Repository metadata refreshed: {} packages in {:.1}s.",
                count, secs
            ),
        }
    }

    pub fn metadata_refresh_offline(self) -> &'static str {
        match self {
            Language::Zh => "离线模式（--offline）下无法更新仓库元数据缓存。",
            Language::En => "Cannot refresh repository metadata in offline mode (--offline).",
        }
    }

    pub fn metadata_refresh_failed(self) -> &'static str {
        match self {
            Language::Zh => "刷新失败：所有仓库均不可达，已保留现有缓存。",
            Language::En => {
                "Refresh failed: all repositories unreachable, existing cache preserved."
            }
        }
    }

    /// 部分源未能在线刷新（回退/彻底失败）——计入失败，非零退出码
    pub fn metadata_refresh_partial(self, fallback: usize, failed: usize) -> String {
        let unsuccessful = fallback + failed;
        match self {
            Language::Zh => format!(
                "⚠ {} 个源刷新失败（在线失败 {}，其中 {} 个连本地缓存也不可用），本次使用本地缓存数据，稍后可重试。",
                unsuccessful, fallback, failed
            ),
            Language::En => format!(
                "⚠ {} sources failed to refresh ({} fell back to local cache, {} had no cache at all), using stale data, retry later.",
                unsuccessful, fallback, failed
            ),
        }
    }

    pub fn no_matches(self) -> &'static str {
        match self {
            Language::Zh => "未找到匹配结果。",
            Language::En => "No matches found.",
        }
    }

    pub fn no_owner(self) -> &'static str {
        match self {
            Language::Zh => "没有软件包拥有此文件。",
            Language::En => "No package owns this file.",
        }
    }

    pub fn dir_shared_by(self, path: &str, count: usize) -> String {
        match self {
            Language::Zh => format!("目录 '{}' 被以下 {} 个软件包共同包含/使用:", path, count),
            Language::En => format!("Directory '{}' is shared by {} packages:", path, count),
        }
    }

    pub fn path_not_found(self, path: &str) -> String {
        match self {
            Language::Zh => format!("错误: 本地文件或路径不存在: '{}'", path),
            Language::En => format!("Error: Local file or path does not exist: '{}'", path),
        }
    }

    pub fn no_files_matched(self) -> &'static str {
        match self {
            Language::Zh => "未找到匹配的文件或路径。",
            Language::En => "No matching file or path found.",
        }
    }

    pub fn not_installed_searching(self, name: &str) -> String {
        match self {
            Language::Zh => format!("软件包 '{}' 本地未安装，正在搜索仓库...", name),
            Language::En => format!(
                "Package '{}' not installed locally, searching repo...",
                name
            ),
        }
    }

    pub fn not_found(self, name: &str) -> String {
        match self {
            Language::Zh => format!("未找到 '{}'。", name),
            Language::En => format!("'{}' not found.", name),
        }
    }

    pub fn not_found_both(self, name: &str) -> String {
        match self {
            Language::Zh => format!("错误: 未找到软件包 '{}'（本地及仓库中均不存在）", name),
            Language::En => format!(
                "Error: Package '{}' not found (neither installed nor in repository)",
                name
            ),
        }
    }

    pub fn not_found_repo(self, name: &str) -> String {
        match self {
            Language::Zh => format!("软件包 '{}' 在仓库中未找到。", name),
            Language::En => format!("Package '{}' not found in repo.", name),
        }
    }

    pub fn no_deps(self, name: &str) -> String {
        match self {
            Language::Zh => format!("包 '{}' 无依赖关系。", name),
            Language::En => format!("Package '{}' has no dependencies.", name),
        }
    }

    pub fn no_rdeps(self, name: &str) -> String {
        match self {
            Language::Zh => format!("未找到 '{}' 的反向依赖。", name),
            Language::En => format!("No reverse dependencies for '{}'.", name),
        }
    }

    pub fn no_changelog(self, name: &str) -> String {
        match self {
            Language::Zh => format!("未找到 '{}' 的变更日志。", name),
            Language::En => format!("No changelog for '{}'.", name),
        }
    }

    pub fn no_changelog_repo(self) -> &'static str {
        match self {
            Language::Zh => "未找到变更日志。",
            Language::En => "No changelog found.",
        }
    }

    pub fn searching_repo_changelog(self) -> &'static str {
        match self {
            Language::Zh => "本地无变更日志，正在搜索仓库...",
            Language::En => "No local changelog, searching repo...",
        }
    }

    pub fn searching_repo_source(self) -> &'static str {
        match self {
            Language::Zh => "本地未找到，正在搜索仓库...",
            Language::En => "Not found locally, searching repo...",
        }
    }

    pub fn found_in_repo(self, name: &str) -> String {
        match self {
            Language::Zh => format!("仓库中找到 {}，请安装后再次查看。", name),
            Language::En => format!("Found {} in repo, install to view files.", name),
        }
    }

    pub fn install_cmd_label(self) -> &'static str {
        match self {
            Language::Zh => "安装命令",
            Language::En => "Install command",
        }
    }

    pub fn no_file_list(self, name: &str) -> String {
        match self {
            Language::Zh => format!("包 '{}' 已安装但无文件列表。", name),
            Language::En => format!("Package '{}' installed but no file list.", name),
        }
    }

    pub fn no_specific_files(self, name: &str) -> String {
        match self {
            Language::Zh => format!("包 '{}' 无专属文件（使用 --all 查看完整列表）。", name),
            Language::En => format!(
                "Package '{}' has no specific files (use --all for full list).",
                name
            ),
        }
    }

    pub fn no_files_found(self) -> &'static str {
        match self {
            Language::Zh => "未找到文件。",
            Language::En => "No files found.",
        }
    }

    pub fn pkg_not_found(self) -> &'static str {
        match self {
            Language::Zh => "未找到软件包。",
            Language::En => "Package not found.",
        }
    }

    pub fn source_not_found(self, name: &str) -> String {
        match self {
            Language::Zh => format!("错误: 未找到软件包或源码包 '{}'（本地及仓库中均不存在）", name),
            Language::En => format!("Error: Package or source package '{}' not found (neither installed nor in repository)", name),
        }
    }

    pub fn binary_packages(self) -> &'static str {
        match self {
            Language::Zh => "包含的二进制包",
            Language::En => "Binary Packages",
        }
    }

    pub fn field_name(self) -> &'static str {
        match self {
            Language::Zh => "名称",
            Language::En => "Name",
        }
    }
    pub fn field_epoch(self) -> &'static str {
        match self {
            Language::Zh => "Epoch",
            Language::En => "Epoch",
        }
    }
    pub fn field_version(self) -> &'static str {
        match self {
            Language::Zh => "版本",
            Language::En => "Version",
        }
    }
    pub fn field_release(self) -> &'static str {
        match self {
            Language::Zh => "发布号",
            Language::En => "Release",
        }
    }
    pub fn field_arch(self) -> &'static str {
        match self {
            Language::Zh => "架构",
            Language::En => "Architecture",
        }
    }
    pub fn field_priority(self) -> &'static str {
        match self {
            Language::Zh => "优先级",
            Language::En => "Priority",
        }
    }
    pub fn field_section(self) -> &'static str {
        match self {
            Language::Zh => "分类",
            Language::En => "Section",
        }
    }
    pub fn field_source(self) -> &'static str {
        match self {
            Language::Zh => "源码包",
            Language::En => "Source",
        }
    }
    pub fn field_maintainer(self) -> &'static str {
        match self {
            Language::Zh => "维护者",
            Language::En => "Maintainer",
        }
    }
    pub fn field_license(self) -> &'static str {
        match self {
            Language::Zh => "许可证",
            Language::En => "License",
        }
    }
    pub fn field_vendor(self) -> &'static str {
        match self {
            Language::Zh => "厂商",
            Language::En => "Vendor",
        }
    }
    pub fn field_summary(self) -> &'static str {
        match self {
            Language::Zh => "摘要",
            Language::En => "Summary",
        }
    }
    pub fn field_description(self) -> &'static str {
        match self {
            Language::Zh => "描述",
            Language::En => "Description",
        }
    }
    pub fn field_homepage(self) -> &'static str {
        match self {
            Language::Zh => "主页",
            Language::En => "Homepage",
        }
    }
    pub fn field_download_size(self) -> &'static str {
        match self {
            Language::Zh => "下载大小",
            Language::En => "Download-Size",
        }
    }
    pub fn field_install_size(self) -> &'static str {
        match self {
            Language::Zh => "安装大小",
            Language::En => "Install-Size",
        }
    }
    pub fn field_provides(self) -> &'static str {
        match self {
            Language::Zh => "提供",
            Language::En => "Provides",
        }
    }

    pub fn dep_depends(self) -> &'static str {
        match self {
            Language::Zh => "依赖",
            Language::En => "Depends",
        }
    }
    pub fn dep_recommends(self) -> &'static str {
        match self {
            Language::Zh => "推荐",
            Language::En => "Recommends",
        }
    }
    pub fn dep_suggests(self) -> &'static str {
        match self {
            Language::Zh => "建议",
            Language::En => "Suggests",
        }
    }
    pub fn dep_conflicts(self) -> &'static str {
        match self {
            Language::Zh => "冲突",
            Language::En => "Conflicts",
        }
    }
    pub fn dep_breaks(self) -> &'static str {
        match self {
            Language::Zh => "破坏",
            Language::En => "Breaks",
        }
    }
    pub fn dep_replaces(self) -> &'static str {
        match self {
            Language::Zh => "替换",
            Language::En => "Replaces",
        }
    }

    pub fn metadata_check(self, ago: &str, timestamp: &str) -> String {
        match self {
            Language::Zh => format!("上次元数据过期检查：{}前，执行于 {}。", ago, timestamp),
            Language::En => format!(
                "Last metadata expiration check: {} ago, on {}.",
                ago, timestamp
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_zh() {
        let l = Language::Zh;
        assert_eq!(l.installed_tag(), "[已安装]");
        assert_eq!(l.repo_tag(), "[未安装]");
        assert!(l.packages_header(5).contains("软件包"));
        assert_eq!(l.field_name(), "名称");
        assert_eq!(l.dep_depends(), "依赖");
        assert_eq!(l.binary_packages(), "包含的二进制包");
    }

    #[test]
    fn test_detect_en() {
        let l = Language::En;
        assert_eq!(l.installed_tag(), "[installed]");
        assert_eq!(l.repo_tag(), "[repo]");
        assert!(l.packages_header(5).contains("Packages"));
        assert_eq!(l.field_name(), "Name");
        assert_eq!(l.dep_depends(), "Depends");
        assert_eq!(l.binary_packages(), "Binary Packages");
    }
}

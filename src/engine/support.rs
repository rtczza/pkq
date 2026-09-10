//! engine 内部共享辅助：时间格式化、元数据 Banner、词边界匹配、
//! 关键词检索、文件归属查询、cache 子命令处理、后端探测。
//!
//! 仅对 `engine` 模块树内可见（`pub(super)`/`pub(crate)`），不对外暴露。

use crate::backend::PkgBackend;
use crate::cli::CacheCmd;
use crate::engine::{Ctx, ExitStatus};
use crate::error::{PkgError, Result};
use crate::i18n::Language;
use crate::model::*;
use crate::{backend, output, path_util};

pub(super) fn lang() -> Language {
    Language::current()
}

// ---------------------------------------------------------------------------
// 元数据 Banner（DNF 风格）
// ---------------------------------------------------------------------------

pub(super) fn format_time_ago(secs: u64) -> String {
    let l = lang();
    if secs < 60 {
        match l {
            Language::Zh => format!("{}秒", secs),
            Language::En => format!("{} second{}", secs, if secs == 1 { "" } else { "s" }),
        }
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        match l {
            Language::Zh => format!("{}分{}秒", m, s),
            Language::En => format!(
                "{} minute{} {} second{}",
                m,
                if m == 1 { "" } else { "s" },
                s,
                if s == 1 { "" } else { "s" }
            ),
        }
    } else if secs < 86400 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        match l {
            Language::Zh => format!("{}小时{}分", h, m),
            Language::En => format!(
                "{} hour{} {} minute{}",
                h,
                if h == 1 { "" } else { "s" },
                m,
                if m == 1 { "" } else { "s" }
            ),
        }
    } else {
        let d = secs / 86400;
        let h = (secs % 86400) / 3600;
        match l {
            Language::Zh => format!("{}天{}小时", d, h),
            Language::En => format!(
                "{} day{} {} hour{}",
                d,
                if d == 1 { "" } else { "s" },
                h,
                if h == 1 { "" } else { "s" }
            ),
        }
    }
}

fn format_full_timestamp(ts: i64) -> String {
    if ts == 0 {
        return String::new();
    }
    let l = lang();
    // libc::localtime_r：按系统时区分解（此前按 UTC 分解导致
    // 中文环境显示比本地时间早 8 小时）
    let (year, month, day, h, m, s, weekday_idx) = local_time_parts(ts);
    match l {
        Language::Zh => {
            let wd = ["日", "一", "二", "三", "四", "五", "六"];
            format!(
                "{}年{:02}月{:02}日 星期{} {:02}:{:02}:{:02}",
                year, month, day, wd[weekday_idx], h, m, s
            )
        }
        Language::En => {
            let wd = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            let mon = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!(
                "{} {} {} {:02}:{:02}:{:02} {}",
                wd[weekday_idx],
                mon[(month - 1) as usize],
                day,
                h,
                m,
                s,
                year
            )
        }
    }
}

/// FFI：libc::localtime_r 按系统时区（TZ）分解 Unix 时间戳。
/// 返回 (年, 月, 日, 时, 分, 秒, 星期索引 0=周日)。单线程进程内线程安全。
fn local_time_parts(ts: i64) -> (i64, u32, u32, u32, u32, u32, usize) {
    use std::os::raw::c_int;
    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,  // 0-11
        tm_year: c_int, // 年-1900
        tm_wday: c_int, // 0=周日
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: i64,
        tm_zone: *const u8,
    }
    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    }
    // SAFETY: localtime_r 是可重入的标准 libc 函数；tm 为栈上零值结构，
    // 指针字段仅由 libc 内部填充（指向静态时区名，进程生命周期内有效）。
    let tm = unsafe {
        let t: i64 = ts;
        let mut tm: Tm = std::mem::zeroed();
        if localtime_r(&t as *const i64 as *const _, &mut tm as *mut Tm).is_null() {
            return (1970, 1, 1, 0, 0, 0, 4);
        }
        tm
    };
    (
        tm.tm_year as i64 + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
        tm.tm_wday.max(0) as usize % 7,
    )
}

pub(super) fn print_metadata_check() {
    let l = lang();
    let mtime = get_repo_metadata_mtime();
    if mtime == 0 {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ago = format_time_ago(now.saturating_sub(mtime));
    let ts = format_full_timestamp(mtime as i64);
    eprintln!("{}", l.metadata_check(&ago, &ts));
}

/// 元数据「上次过期检查」时间源，优先级：
/// 1. pkq 自身缓存目录树的最大 mtime —— 真正语义（本次 cache update /
///    在线刷新 / 磁盘缓存落盘都会 touch），修复 RPM 端误显系统 dnf
///    旧缓存时间的问题；
/// 2. 首次运行（pkq 缓存尚未建立）时，按系统回退到发行版原生缓存目录。
fn get_repo_metadata_mtime() -> u64 {
    use std::path::Path;
    // 1. pkq 自身缓存（最高优先级）
    let own = crate::cache::cache_dir_mtime();
    if own > 0 {
        return own;
    }
    // 2a. RPM：发行版原生缓存（仅 pkq 缓存为空时）
    let candidates: &[&str] = &["/var/cache/dnf", "/var/cache/yum"];
    for c in candidates {
        let p = shellexpand(c);
        if Path::new(&p).exists() {
            if let Ok(m) = std::fs::metadata(&p) {
                if let Ok(t) = m.modified() {
                    let s = t
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if s > 0 {
                        return s;
                    }
                }
            }
        }
    }
    // 2b. DEB: apt lists latest mtime
    if let Ok(entries) = std::fs::read_dir("/var/lib/apt/lists") {
        let mut max_mtime: u64 = 0;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with("_Packages") || name_str.ends_with("_Contents-") {
                if let Ok(m) = entry.metadata() {
                    if let Ok(t) = m.modified() {
                        let s = t
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if s > max_mtime {
                            max_mtime = s;
                        }
                    }
                }
            }
        }
        if max_mtime > 0 {
            return max_mtime;
        }
    }
    // 2c. DEB: periodic update stamp
    let stamp = Path::new("/var/lib/apt/periodic/update-success-stamp");
    if let Ok(m) = std::fs::metadata(stamp) {
        if let Ok(t) = m.modified() {
            return t
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }
    }
    0
}

fn shellexpand(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// 每条命令最多打印一次元数据 Banner（本地命中不触发仓库的分支不打印）
pub(super) fn show_banner(shown: &mut bool) {
    if !*shown {
        print_metadata_check();
        *shown = true;
    }
}

// ---------------------------------------------------------------------------
// 通用小工具
// ---------------------------------------------------------------------------

pub(super) fn install_cmd(backend: &dyn PkgBackend, name: &str) -> String {
    match backend.system_type() {
        PackageSystem::Rpm => format!("dnf install {}", name),
        PackageSystem::Deb => format!("apt-get install {}", name),
    }
}

/// 词边界匹配：关键词在 haystack 中以独立词出现（前后均非字母数字），用于
/// 搜索相关性分层（"sudo" 不应因 "sudoku" 的子串关系被视为摘要强匹配）
pub(super) fn word_boundary_contains(hay_lower: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = hay_lower[start..].find(needle_lower) {
        let abs = start + pos;
        let end = abs + needle_lower.len();
        let before_ok = hay_lower[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        let after_ok = hay_lower[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        start = end;
    }
    false
}

/// 快速补齐源码包名：本地未标明时默认源码包名同二进制包名，绝不主动扫描远程仓库
pub(super) fn supplement_source_pkg(pkg: &mut PkgMetadata) {
    if pkg.source_pkg.is_none() {
        pkg.source_pkg = Some(pkg.name.clone());
    }
}

pub(super) fn is_path_query(pattern: &str) -> bool {
    pattern.starts_with('/') || path_util::contains_glob(pattern)
}

pub(super) fn keyword_search(
    backend: &dyn PkgBackend,
    pattern: &str,
    use_regex: bool,
    source: PackageSource,
    cfg: &CacheConfig,
) -> Result<Vec<SearchResult>> {
    let packages = backend.search_packages(pattern, source, cfg)?;
    let pl = pattern.to_lowercase();
    Ok(packages
        .iter()
        .filter_map(|p| {
            // 仅匹配 name + summary（与 dnf search 语义对齐）；description 匹配
            // 噪音过大（如描述正文偶然出现关键词导致整段描述刷屏），不再纳入
            let matched = if use_regex {
                regex::Regex::new(pattern)
                    .ok()
                    .map(|re| re.is_match(&p.name) || re.is_match(&p.summary))
                    .unwrap_or(false)
            } else {
                p.name.to_lowercase().contains(&pl) || p.summary.to_lowercase().contains(&pl)
            };
            if matched {
                Some(SearchResult {
                    pkg_name: p.name.clone(),
                    matched_text: p.summary.clone(),
                    match_type: "keyword".to_string(),
                    source: match source {
                        PackageSource::Installed => "installed",
                        PackageSource::Repo => "repo",
                    }
                    .to_string(),
                })
            } else {
                None
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// 文件归属查询（owns / search 路径查询共用）
// ---------------------------------------------------------------------------

pub(super) fn handle_file_query(
    ctx: &mut Ctx,
    pattern: &str,
    source: PackageSource,
    show_all: bool,
) -> Result<ExitStatus> {
    let l = lang();
    let Ctx {
        backend, cfg, fmt, ..
    } = ctx;
    if path_util::contains_glob(pattern) {
        let regex_pattern = path_util::glob_to_regex(pattern);
        let local_results = backend.search_by_pattern(&regex_pattern, true, source, cfg)?;
        if local_results.is_empty() {
            eprintln!("{}", l.no_files_matched());
            return Ok(ExitStatus::NotFound);
        }
        let mut grouped: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for r in &local_results {
            grouped
                .entry(r.pkg_name.clone())
                .or_default()
                .push(r.matched_text.clone());
        }
        output::print_file_owners_grouped(&grouped, *fmt)?;
        return Ok(ExitStatus::Hit);
    }
    let candidates = path_util::normalize_path(pattern);
    let resolved_path = &candidates[0];
    // P0-2：存在性校验仅针对本地查询；仓库查询的核心场景就是查未安装的文件
    if source == PackageSource::Installed && !std::path::Path::new(resolved_path).exists() {
        eprintln!("{}", l.path_not_found(pattern));
        return Ok(ExitStatus::NotFound);
    }
    let metadata = std::fs::metadata(resolved_path);
    let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let mut owners = Vec::new();
    for candidate in &candidates {
        owners = backend.find_file_owner(candidate, source, cfg)?;
        if !owners.is_empty() {
            break;
        }
    }
    if owners.is_empty() {
        eprintln!("{}", l.no_owner());
        return Ok(ExitStatus::NotFound);
    }
    // P0-9：标注来源
    let source_str = match source {
        PackageSource::Installed => "installed",
        PackageSource::Repo => "repo",
    };
    let total = owners.len();
    // 目录共享/单文件归属者过多时默认限流展示；--all 查看全部（需要前 N 条请配合 | head -N）
    let default_limit = crate::backend::common::DEFAULT_PAGE_LIMIT;
    let truncated = !show_all && total > default_limit;
    let shown: Vec<String> = if truncated {
        owners.into_iter().take(default_limit).collect()
    } else {
        owners
    };
    let items: Vec<(String, String)> = shown
        .into_iter()
        .map(|n| (n, source_str.to_string()))
        .collect();
    if is_dir && items.len() > 1 {
        output::print_directory_owners(&items, resolved_path, total, *fmt)?;
    } else {
        output::print_file_owners(&items, resolved_path, *fmt)?;
    }
    if truncated {
        eprintln!("{}", l.limit_note(default_limit));
    }
    Ok(ExitStatus::Hit)
}

// ---------------------------------------------------------------------------
// cache 子命令（P1-4）
// ---------------------------------------------------------------------------

pub(super) fn handle_cache(ctx: &mut Ctx, cmd: &CacheCmd) -> Result<ExitStatus> {
    let Ctx {
        backend, cfg, fmt, ..
    } = ctx;
    match cmd {
        CacheCmd::Status => {
            let items = crate::cache::cache_status();
            let total: u64 = items.iter().map(|(_, s)| s).sum();
            let dir = dirs::cache_dir().map(|d| d.join("pkq")).unwrap_or_default();
            output::print_cache_status(&dir.to_string_lossy(), &items, total, *fmt)?;
            Ok(ExitStatus::Hit)
        }
        CacheCmd::Update => {
            // 强制刷新仓库元数据缓存（等价于全局 --refresh 的显式入口）
            if cfg.offline_mode {
                eprintln!("{}", lang().metadata_refresh_offline());
                return Ok(ExitStatus::NotFound);
            }
            let start = std::time::Instant::now();
            match backend.refresh_metadata(cfg) {
                Ok(report) => {
                    let secs = start.elapsed().as_secs_f64();
                    println!("{}", lang().metadata_refreshed(report.package_count, secs));
                    // 回退/彻底失败均计入失败：cache update 的目标是刷新到最新
                    if report.stats.all_online() {
                        Ok(ExitStatus::Hit)
                    } else {
                        let s = report.stats;
                        eprintln!("{}", lang().metadata_refresh_partial(s.fallback, s.failed));
                        Ok(ExitStatus::NotFound)
                    }
                }
                Err(_) => {
                    eprintln!("{}", lang().metadata_refresh_failed());
                    Ok(ExitStatus::NotFound)
                }
            }
        }
        CacheCmd::Clean { target, yes } => {
            // 先校验目标合法性，再进入确认/清理流程
            if !matches!(target.as_str(), "all" | "index" | "repos" | "contents") {
                return Err(PkgError::InvalidArgument(format!(
                    "未知清理目标: {}（可选 all|index|repos|contents）",
                    target
                )));
            }
            // 确认提示展示的是清理目标自身的大小，而非全部缓存
            let target_size = crate::cache::cache_target_size(target);
            if !*yes {
                let l = lang();
                eprintln!("{}", l.cache_clean_confirm(target, target_size));
                return Ok(ExitStatus::NotFound);
            }
            let freed = crate::cache::cache_clean(target)?;
            let l = lang();
            println!("{}", l.cache_cleaned(freed));
            Ok(ExitStatus::Hit)
        }
    }
}

// ---------------------------------------------------------------------------
// 后端探测
// ---------------------------------------------------------------------------

pub(super) fn detect_backend() -> Result<Box<dyn PkgBackend>> {
    let system = detect_system();
    tracing::debug!("detected package system: {:?}", system);
    match system {
        Some(PackageSystem::Deb) => Ok(Box::new(backend::deb::DebBackend::new())),
        Some(PackageSystem::Rpm) => Ok(Box::new(backend::rpm::RpmBackend::new())),
        None => Err(PkgError::InvalidArgument(
            "不支持的系统：既未找到 RPM 也未找到 DEB".into(),
        )),
    }
}

/// 依据 os-release 判定包系统；UOS 等双系发行版（ID 相同、无 ID_LIKE）返回 None，
/// 由路径探测兜底（RPM 数据库优先于 dpkg，兼容混合环境）。
pub(super) fn detect_system() -> Option<PackageSystem> {
    if let Some(sys) = system_from_os_release() {
        return Some(sys);
    }
    // 路径探测：RPM 数据库三种形态（sqlite / NDB / BDB），再查 dpkg
    let rpm_dbs = [
        "/var/lib/rpm/rpmdb.sqlite",
        "/var/lib/rpm/Packages.db",
        "/var/lib/rpm/Packages",
    ];
    if rpm_dbs.iter().any(|p| std::path::Path::new(p).exists()) {
        return Some(PackageSystem::Rpm);
    }
    if std::path::Path::new("/var/lib/dpkg/status").exists() {
        return Some(PackageSystem::Deb);
    }
    None
}

fn system_from_os_release() -> Option<PackageSystem> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;
    let mut id = String::new();
    let mut id_like = String::new();
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("ID=") {
            id = v.trim_matches('"').to_lowercase();
        } else if let Some(v) = line.strip_prefix("ID_LIKE=") {
            id_like = v.trim_matches('"').to_lowercase();
        }
    }
    system_from_id_tokens(&id, &id_like)
}

/// 依据 ID / ID_LIKE 词元判定包系统族；无法判定（如 UOS）返回 None
pub(super) fn system_from_id_tokens(id: &str, id_like: &str) -> Option<PackageSystem> {
    let tokens: Vec<&str> = id_like
        .split_whitespace()
        .chain(std::iter::once(id))
        .collect();
    const DEB_FAMILY: &[&str] = &["debian", "ubuntu", "deepin"];
    const RPM_FAMILY: &[&str] = &[
        "fedora",
        "rhel",
        "centos",
        "suse",
        "opensuse",
        "anolis",
        "opencloudos",
        "euleros",
        "openeuler",
        "fedora-like",
    ];
    if tokens.iter().any(|t| DEB_FAMILY.contains(t)) {
        return Some(PackageSystem::Deb);
    }
    if tokens.iter().any(|t| RPM_FAMILY.contains(t)) {
        return Some(PackageSystem::Rpm);
    }
    None
}

// ---------------------------------------------------------------------------
// list 公共目录过滤
// ---------------------------------------------------------------------------

const COMMON_DIRS: &[&str] = &[
    "/",
    "/.",
    "/..",
    "/usr",
    "/usr/bin",
    "/usr/sbin",
    "/usr/lib",
    "/usr/lib32",
    "/usr/lib64",
    "/usr/libx32",
    "/usr/share",
    "/usr/share/doc",
    "/usr/share/man",
    "/usr/share/locale",
    "/usr/share/lintian",
    "/usr/share/bug",
    "/etc",
    "/etc/default",
    "/etc/init.d",
    "/bin",
    "/sbin",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    "/var",
    "/var/lib",
    "/var/cache",
    "/opt",
];

pub(super) fn is_common_dir(path: &str) -> bool {
    let p = path.trim_end_matches('/');
    p.is_empty() || COMMON_DIRS.contains(&p) || COMMON_DIRS.contains(&path)
}

pub(super) fn filter_common_dirs(files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|f| !is_common_dir(f))
        .cloned()
        .collect()
}

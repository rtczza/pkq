//! 双后端（DEB/RPM）共享的常量与轻量辅助。
//!
//! 目标：消除成对重复逻辑（限流阈值、来源标签映射、网络错误压缩等），
//! 后续如需泛型化仓库缓存骨架，也从这里起步。

use crate::model::PkgMetadata;

/// 长列表（rdeps / owns 目录共享等）默认限流条数，`--all` 可解除。
pub const DEFAULT_PAGE_LIMIT: usize = 50;

/// 获取互斥锁并容忍 poison：即使先前持锁线程 panic，后续仍能取回内部数据，
/// 避免 `lock().unwrap()` 把一次局部 panic 升级为永久性二次 panic。
///
/// 安全性依据：本项目的 `Mutex` 仅保护可重建的缓存/统计（`mirror_cache` /
/// `last_refresh`），不存在需要跨线程维持的一致性不变量，替换为内部数据是安全的。
pub(crate) fn lock_recover<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 噪声文件扩展名（图标/网页等），关键词路径检索时过滤以避免刷屏。
pub const NOISE_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".bmp", ".html", ".htm", ".css",
];

/// 小写路径是否以噪声扩展名结尾。
pub(crate) fn has_noise_extension(path_lower: &str) -> bool {
    NOISE_EXTENSIONS.iter().any(|ext| path_lower.ends_with(ext))
}

/// 字节级噪声扩展名判断（Contents 原始字节扫描用，大小写不敏感）。
pub(crate) fn is_noise_ext_bytes(path: &[u8]) -> bool {
    let from = path.len().saturating_sub(6);
    let tail = &path[from..];
    NOISE_EXTENSIONS.iter().any(|ext| {
        let ext = ext.as_bytes();
        tail.len() >= ext.len() && tail[tail.len() - ext.len()..].eq_ignore_ascii_case(ext)
    })
}

/// 路径段匹配：整路径精确（`pattern_lower` 以 `/` 开头）或按段前缀/后缀带边界匹配。
/// 双端（DEB Contents / RPM 文件表 / `search_by_pattern`）共用，避免语义漂移。
pub(crate) fn path_segment_match(path: &str, pattern_lower: &str) -> bool {
    if pattern_lower.starts_with('/') {
        return path.trim_end_matches('/') == pattern_lower.trim_end_matches('/');
    }
    let path_lower = path.to_lowercase();
    for seg in path_lower.split('/') {
        if seg == pattern_lower {
            return true;
        }
        if let Some(rest) = seg.strip_prefix(pattern_lower) {
            if rest.is_empty()
                || rest.starts_with('.')
                || rest.starts_with('-')
                || rest.starts_with('_')
            {
                return true;
            }
        }
        if let Some(rest) = seg.strip_suffix(pattern_lower) {
            if rest.is_empty() || rest.ends_with('.') || rest.ends_with('-') || rest.ends_with('_')
            {
                return true;
            }
        }
    }
    false
}

/// 架构优先级权重：主架构 < 通用（`noarch`/`all`） < 32 位 < 其他。双端共享。
pub(crate) fn arch_weight(arch: &str) -> u8 {
    match arch {
        "amd64" | "x86_64" | "aarch64" => 0,
        "noarch" | "all" => 1,
        "i386" | "armhf" | "armel" => 2,
        _ => 3,
    }
}

/// 单个仓库源的一次刷新结果来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// 在线获取成功（含 HTTP 304 未变化 / ETag / TTL 命中）
    Online,
    /// 在线失败，已回退本地缓存（数据可用但可能过期；携带压缩后的原因）
    LocalFallback(String),
}

/// 将冗长的网络错误压缩为可读短语（去除 URL 等噪音，label 已标识源）。
pub fn brief_network_reason(err: &str) -> String {
    let s = err;
    if s.contains("timed out") {
        return "响应超时".into();
    }
    if s.contains("unexpected end of file") {
        return "连接中断".into();
    }
    if s.contains("connection refused") {
        return "连接被拒绝".into();
    }
    if s.contains("dns error") || s.contains("failed to lookup") || s.contains("resolve") {
        return "DNS 解析失败".into();
    }
    if s.contains("status code") {
        if let Some(pos) = s.find("status code ") {
            let rest = &s[pos + "status code ".len()..];
            let code: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !code.is_empty() {
                let hint = match code.as_str() {
                    "401" | "403" => "（认证失败）",
                    "404" => "（路径不存在）",
                    "5" if code.starts_with('5') => "（服务端错误）",
                    _ => "",
                };
                return format!("HTTP {}{}", code, hint);
            }
        }
    }
    // 兜底：截断
    s.chars().take(50).collect()
}

/// 一次 `cache update` 的分源统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefreshStats {
    /// 本次参与刷新的源任务总数
    pub total: usize,
    /// 在线刷新成功（含 304/ETag/TTL 命中）
    pub online: usize,
    /// 在线失败但已回退本地缓存——**计入失败**（数据可用但可能过期）
    pub fallback: usize,
    /// 彻底失败（在线失败且本地缓存也不可用）
    pub failed: usize,
}

impl RefreshStats {
    /// 未成功在线刷新的源数（回退 + 彻底失败）
    pub fn unsuccessful(&self) -> usize {
        self.fallback + self.failed
    }
    /// 全部源在线成功
    pub fn all_online(&self) -> bool {
        self.total > 0 && self.unsuccessful() == 0
    }
}

/// `cache update` 的整体结果。
#[derive(Debug, Clone)]
pub struct RefreshReport {
    pub stats: RefreshStats,
    /// 刷新后缓存中的软件包总数
    pub package_count: usize,
}

/// 后端一次全量刷新的中间产物（fetch_all_repos / 仓库循环的返回值）。
pub struct FetchSummary {
    pub packages: Vec<PkgMetadata>,
    pub stats: RefreshStats,
    pub package_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn lock_recover_survives_poison() {
        let mutex = Arc::new(Mutex::new(7u32));
        let cloned = Arc::clone(&mutex);
        let _ = std::thread::spawn(move || {
            let _guard = cloned.lock().unwrap();
            panic!("intentional poison");
        })
        .join();
        assert!(mutex.is_poisoned());
        assert_eq!(*lock_recover(&mutex), 7);
    }

    #[test]
    fn arch_weight_orders_primary_over_generic() {
        assert_eq!(arch_weight("amd64"), 0);
        assert_eq!(arch_weight("x86_64"), 0);
        assert_eq!(arch_weight("noarch"), 1);
        assert_eq!(arch_weight("all"), 1);
        assert_eq!(arch_weight("i386"), 2);
        assert_eq!(arch_weight("riscv64"), 3);
    }

    #[test]
    fn shared_path_and_noise_helpers() {
        assert!(path_segment_match("/usr/bin/unzip", "unzip"));
        assert!(path_segment_match("/usr/bin/unzip-bin", "unzip"));
        assert!(!path_segment_match("/usr/bin/unzip2", "unzip"));
        assert!(path_segment_match("/usr/bin/lunzip", "/usr/bin/lunzip"));
        assert!(has_noise_extension("/usr/share/icons/a.png"));
        assert!(!has_noise_extension("/usr/bin/pngview"));
        assert!(is_noise_ext_bytes(b"/usr/share/x.PNG"));
    }
}

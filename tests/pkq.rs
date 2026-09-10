//! 集成冒烟测试：验证 lib crate 公共 API 的核心行为。
//!
//! 覆盖面：模型序列化往返、错误显示、路径工具、i18n 文案、
//! 缓存目录探测、后端探测。均为纯函数级验证，不依赖系统包管理器状态。

use pkq::error::PkgError;
use pkq::model::*;
use pkq::path_util;

// ---------------------------------------------------------------------------
// model：序列化往返
// ---------------------------------------------------------------------------

#[test]
fn pkg_metadata_serde_roundtrip() {
    let pkg = PkgMetadata {
        name: "xz".into(),
        version: "5.4.7".into(),
        release: "8.uos25".into(),
        arch: "x86_64".into(),
        summary: "compression tool".into(),
        requires: vec![Dependency {
            name: "libc".into(),
            version: None,
            flags: None,
            is_alternative: false,
        }],
        ..Default::default()
    };
    let json = serde_json::to_string(&pkg).expect("serialize");
    let back: PkgMetadata = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.name, "xz");
    assert_eq!(back.requires.len(), 1);
    assert_eq!(back.requires[0].name, "libc");
}

#[test]
fn search_result_serde_roundtrip() {
    let r = SearchResult {
        pkg_name: "sudo".into(),
        matched_text: "super user".into(),
        match_type: "keyword".into(),
        source: "installed".into(),
    };
    let json = serde_json::to_string(&r).expect("serialize");
    let back: SearchResult = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, r);
}

#[test]
fn cache_config_defaults() {
    let cfg = CacheConfig::default();
    assert_eq!(cfg.ttl_secs, 86400);
    assert!(!cfg.force_refresh);
    assert!(!cfg.offline_mode);
}

// ---------------------------------------------------------------------------
// error：显示格式
// ---------------------------------------------------------------------------

#[test]
fn pkg_error_display_variants() {
    assert_eq!(
        PkgError::NetworkError("timeout".into()).to_string(),
        "Network error: timeout"
    );
    assert_eq!(
        PkgError::InvalidArgument("bad".into()).to_string(),
        "Invalid argument: bad"
    );
}

// ---------------------------------------------------------------------------
// path_util：glob 与路径归一化
// ---------------------------------------------------------------------------

#[test]
fn glob_to_regex_documented_behavior() {
    // 已知现状：glob_to_regex 生成【无锚定】正则（* 跨段匹配），
    // 属宽松子串语义——owns 通配符查询依赖该行为（历史契约，勿随意收紧）
    let re_str = path_util::glob_to_regex("*/bin/bash");
    let compiled = regex::Regex::new(&re_str).expect("valid regex");
    assert!(compiled.is_match("/usr/bin/bash"));
    assert!(compiled.is_match("/usr/bin/bashbug")); // 宽松点：子串命中
}

#[test]
fn glob_match_is_segment_bounded() {
    // glob_match 为逐字符有界匹配：* 不跨 `/`，bashbug 不被 */bin/bash 命中
    assert!(path_util::glob_match("*/bin/bash", "/usr/bin/bash"));
    assert!(path_util::glob_match("/bin/*", "/bin/bash"));
    assert!(!path_util::glob_match("*/bin/bash", "/usr/bin/bashbug"));
    assert!(path_util::glob_match("*", "anything"));
}

#[test]
fn normalize_path_resolves_relative() {
    let candidates = path_util::normalize_path("./Cargo.toml");
    assert!(!candidates.is_empty());
    assert!(candidates[0].starts_with('/'));
}

// ---------------------------------------------------------------------------
// i18n：双语标签
// ---------------------------------------------------------------------------

#[test]
fn i18n_tags_distinct_per_language() {
    let zh = pkq::i18n::Language::Zh;
    let en = pkq::i18n::Language::En;
    assert_eq!(zh.installed_tag(), "[已安装]");
    assert_eq!(en.installed_tag(), "[installed]");
    assert!(zh.metadata_check("1分", "T").contains("上次元数据过期检查"));
    assert!(en
        .metadata_check("1m", "T")
        .contains("Last metadata expiration"));
}

// ---------------------------------------------------------------------------
// backend::common：网络错误压缩与刷新统计
// ---------------------------------------------------------------------------

#[test]
fn brief_network_reason_compresses_noise() {
    use pkq::backend::common::brief_network_reason;
    assert_eq!(
        brief_network_reason("Network error: IO error: timed out reading response"),
        "响应超时"
    );
    assert_eq!(
        brief_network_reason("Failed to fetch https://x: status code 401"),
        "HTTP 401（认证失败）"
    );
}

#[test]
fn refresh_stats_semantics() {
    use pkq::backend::common::RefreshStats;
    let all_ok = RefreshStats {
        total: 18,
        online: 18,
        fallback: 0,
        failed: 0,
    };
    assert!(all_ok.all_online());

    let partial = RefreshStats {
        total: 18,
        online: 10,
        fallback: 6,
        failed: 2,
    };
    assert!(!partial.all_online());
    assert_eq!(partial.unsuccessful(), 8);
}

#[test]
fn fetch_outcome_distinguishes_origin() {
    use pkq::backend::common::FetchOutcome;
    let online: FetchOutcome = FetchOutcome::Online;
    let fallback = FetchOutcome::LocalFallback("HTTP 401".into());
    assert_ne!(online, fallback);
    match fallback {
        FetchOutcome::LocalFallback(reason) => assert!(reason.contains("401")),
        _ => panic!("expected fallback"),
    }
}

// ---------------------------------------------------------------------------
// error / exit status
// ---------------------------------------------------------------------------

#[test]
fn exit_status_equality() {
    use pkq::engine::ExitStatus;
    assert_eq!(ExitStatus::Hit, ExitStatus::Hit);
    assert_ne!(ExitStatus::Hit, ExitStatus::NotFound);
}

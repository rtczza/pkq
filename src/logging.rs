//! 统一日志初始化（`tracing`）。
//!
//! 默认级别 `WARN`，`-v` 提升到 `DEBUG`，`-vv` 提升到 `TRACE`。输出到 stderr，
//! 且**关闭时间/目标/级别/ANSI**，以便 `tracing::warn!("Warning: ...")` 的正文
//! 与既有 `eprintln!` 警告逐字一致（不改变用户在默认级别看到的文本）。

use tracing::Level;

/// 将 `-v` 次数映射为日志级别：0=WARN，1=DEBUG，>=2=TRACE。
pub fn level_for_verbosity(verbosity: u8) -> Level {
    match verbosity {
        0 => Level::WARN,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    }
}

/// 初始化全局 tracing 订阅者。重复调用安全（`try_init` 忽略已初始化错误）。
pub fn init(verbosity: u8) {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .with_level(false)
        .with_ansi(false)
        .with_max_level(level_for_verbosity(verbosity))
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_maps_to_levels() {
        assert_eq!(level_for_verbosity(0), Level::WARN);
        assert_eq!(level_for_verbosity(1), Level::DEBUG);
        assert_eq!(level_for_verbosity(2), Level::TRACE);
        assert_eq!(level_for_verbosity(9), Level::TRACE);
    }
}

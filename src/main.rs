use clap::Parser;
use pkq::cli::Cli;
use pkq::engine;
use pkq::error::PkgError;
use pkq::model;

#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: signal() 是异步信号安全的系统调用；此处仅将 SIGPIPE 恢复为
    // 默认处置（进程终止），避免 Rust 运行时忽略 SIGPIPE 导致管道写入
    // 返回错误而非进程终止的默认 Unix 行为。无并发窗口（main 最早执行）。
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() {
    reset_sigpipe();
    let cli = Cli::parse();
    pkq::logging::init(cli.verbose);
    let cache_cfg = model::CacheConfig {
        ttl_secs: cli.cache_ttl,
        force_refresh: cli.refresh,
        offline_mode: cli.offline,
    };

    let output_format = cli.output;
    pkq::output::set_json_compact(cli.compact);
    let result = engine::run(&cli.command, &cache_cfg, output_format);

    match result {
        Ok(engine::ExitStatus::Hit) => {}
        Ok(engine::ExitStatus::NotFound) => {
            if !cli.legacy_exit_code {
                std::process::exit(1);
            }
        }
        Err(e) => {
            if let PkgError::IoError(ref msg) = e {
                if msg.contains("Broken pipe") {
                    std::process::exit(0);
                }
            }
            eprintln!("Error: {}", e);
            std::process::exit(if cli.legacy_exit_code { 0 } else { 2 });
        }
    }
}

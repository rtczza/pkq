use clap::{Parser, ValueEnum};
use clap_complete::engine::{ArgValueCompleter, PathCompleter};

use crate::completion::{cache_target_completer, package_completer, search_pattern_completer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Parser)]
#[command(name = "pkq", author, version, about = "跨发行版 Linux 软件包查询工具")]
pub struct Cli {
    #[arg(short, long, global = true, value_enum, default_value_t = OutputFormat::Human, help = "输出格式")]
    pub output: OutputFormat,

    #[arg(
        long,
        global = true,
        help = "JSON 输出压缩为单行（默认美化缩进）/ Compact single-line JSON"
    )]
    pub compact: bool,

    #[arg(
        short = 'v',
        long,
        global = true,
        action = clap::ArgAction::Count,
        help = "增加日志详细度（-v DEBUG，-vv TRACE）/ Increase log verbosity"
    )]
    pub verbose: u8,

    #[arg(long, global = true, help = "强制刷新仓库元数据缓存")]
    pub refresh: bool,

    #[arg(long, global = true, help = "离线模式，仅使用本地缓存")]
    pub offline: bool,

    #[arg(
        long,
        global = true,
        default_value_t = 86400,
        help = "缓存 TTL（秒，默认 86400）"
    )]
    pub cache_ttl: u64,

    #[arg(
        long,
        global = true,
        help = "兼容模式：无论结果如何均以 0 退出（v0.1 行为）"
    )]
    pub legacy_exit_code: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand)]
pub enum Commands {
    /// 显示软件包详细信息（名称、版本、描述、大小、许可证等）
    #[command(name = "info")]
    Info {
        #[arg(add = ArgValueCompleter::new(package_completer))]
        name: String,
        #[arg(long, help = "查询仓库而非本地已安装")]
        repo: bool,
    },
    /// 列出软件包包含的文件
    #[command(name = "list")]
    List {
        #[arg(add = ArgValueCompleter::new(package_completer))]
        name: String,
        #[arg(long, help = "查询仓库")]
        repo: bool,
        #[arg(short = 'a', long, help = "显示包含公共目录的完整列表")]
        all: bool,
    },
    /// 查找文件归属哪个软件包（支持绝对/相对路径和通配符）
    #[command(name = "owns")]
    Owns {
        /// 文件路径或 Glob 模式（如 /bin/unzip 或 */bin/unzip）
        #[arg(add = ArgValueCompleter::new(PathCompleter::any()))]
        path: String,
        #[arg(long, help = "在仓库中查找")]
        repo: bool,
        #[arg(
            short = 'a',
            long,
            help = "展示全部结果，不限流 / Show all results without truncation"
        )]
        all: bool,
    },
    /// 显示软件包的依赖关系
    #[command(name = "deps")]
    Deps {
        #[arg(add = ArgValueCompleter::new(package_completer))]
        name: String,
        #[arg(long, help = "查询仓库")]
        repo: bool,
    },
    /// 显示反向依赖（哪些包依赖此包）
    #[command(name = "rdeps")]
    RDeps {
        #[arg(add = ArgValueCompleter::new(package_completer))]
        name: String,
        #[arg(
            long,
            help = "仅查询仓库中的反向依赖",
            conflicts_with = "installed_only"
        )]
        repo: bool,
        #[arg(long, help = "仅显示本地已安装的反向依赖")]
        installed_only: bool,
        #[arg(
            short = 'a',
            long,
            help = "展示全部结果，不限流 / Show all results without truncation"
        )]
        all: bool,
    },
    /// 搜索软件包名称和描述（以 / 开头或含通配符时等价于 owns）
    #[command(name = "search")]
    Search {
        /// 搜索关键词（以 / 开头或含 * ? 则转至文件归属查询）
        #[arg(add = ArgValueCompleter::new(search_pattern_completer))]
        pattern: String,
        #[arg(long, help = "使用正则表达式匹配 / Use regex matching")]
        regex: bool,
        #[arg(
            long,
            help = "仅搜索包名和描述，跳过文件索引 / Search names and descriptions only, skip file index"
        )]
        names_only: bool,
        #[arg(long, help = "仅搜索文件路径 / Search file paths only")]
        files_only: bool,
        #[arg(
            short = 'a',
            long = "all",
            alias = "all-files",
            help = "展示全部结果，不限流 / Show all results without truncation"
        )]
        all_files: bool,
        #[arg(
            long,
            help = "每包最大文件结果条数（默认 5，0=不限） / Max file results per package (default 5, 0=unlimited)",
            default_value_t = 5
        )]
        max_files: usize,
        #[arg(
            short = 'i',
            long = "installed",
            alias = "installed-only",
            help = "仅显示本地已安装的包 / Show only locally installed packages"
        )]
        installed: bool,
        #[arg(long, help = "搜索仓库 / Search repository")]
        repo: bool,
    },
    /// 源码包与二进制包互查（自动识别输入是源码包名还是二进制包名）
    #[command(name = "source")]
    Source {
        #[arg(add = ArgValueCompleter::new(package_completer))]
        name: String,
        #[arg(long, help = "查询仓库")]
        repo: bool,
    },
    /// 显示软件包变更日志
    #[command(name = "changelog")]
    Changelog {
        #[arg(add = ArgValueCompleter::new(package_completer))]
        name: String,
        #[arg(long, help = "查询仓库")]
        repo: bool,
    },
    /// 缓存管理
    #[command(name = "cache")]
    Cache {
        #[command(subcommand)]
        cmd: CacheCmd,
    },
}

#[derive(clap::Subcommand)]
pub enum CacheCmd {
    /// 显示各缓存文件的磁盘占用
    #[command(name = "status")]
    Status,
    /// 强制刷新仓库元数据缓存
    #[command(name = "update")]
    Update,
    /// 清理缓存（默认 all；可选 index|repos|contents）
    #[command(name = "clean")]
    Clean {
        /// 清理目标: all | index | repos | contents
        #[arg(default_value = "all", add = ArgValueCompleter::new(cache_target_completer))]
        target: String,
        #[arg(long, help = "跳过确认直接删除")]
        yes: bool,
    },
}

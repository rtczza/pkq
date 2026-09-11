# pkq

> 跨发行版（DEB / RPM）Linux 软件包统一查询工具 —— 一套命令，两种生态。

[![Crates.io](https://img.shields.io/crates/v/pkq.svg)](https://crates.io/crates/pkq)
[![License](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![CI](https://github.com/rtczza/pkq/actions/workflows/ci.yml/badge.svg)](https://github.com/rtczza/pkq/actions/workflows/ci.yml)
[![Platform](https://img.shields.io/badge/platform-Linux-lightgrey)](#平台支持)

`pkq`（**P**ac**k**age **Q**uery）统一封装 `dpkg/apt` 与 `rpm/dnf` 两套包管理生态，
提供一致的查询语法、双语（中/英）排版与统一的 JSON 输出、退出码语义。

## 特性

- **双后端统一**：DEB（dpkg / apt / Contents 索引）与 RPM（rpmdb 三格式原生解析 / repodata）
- **九大子命令**：`info` `list` `owns` `deps` `rdeps` `search` `source` `changelog` `cache`
- **语义对齐原生工具**：`rdeps` 与 `dnf repoquery --whatrequires` 结果一致（强依赖 + 能力集匹配）
- **多级缓存**：HTTP ETag/TTL → 解析结果 postcard → mmap Contents 检索；离线自动降级
- **机器可读**：`--output json` 统一结构化输出；退出码 `0/1/2`（命中/未找到/错误）
- **中文友好**：运行时中英切换、CJK 宽度对齐排版

## 安装

```bash
# 从源码构建（Rust 1.89+，见 Cargo.toml 的 rust-version）
cargo install --path .
# 或直接使用构建产物
cargo build --release && ./target/release/pkq --help
```

## 快速上手

```bash
pkq info bash              # 软件包详情（本地优先，未装自动查仓库）
pkq list xz-utils          # 文件列表
pkq owns /usr/bin/sudo     # 文件归属（支持通配符 */bin/unzip）
pkq deps sudo              # 依赖（RPM 自动反查 .so 能力到真实包名）
pkq rdeps bash             # 反向依赖（与 dnf repoquery --whatrequires 对齐）
pkq search unzip           # 关键词 + 文件路径双索引搜索
pkq source xz-utils        # 源码包 ↔ 二进制包互查
pkq cache status|update|clean
```

任意命令加 `--output json` 获得结构化输出；`--all` 查看完整列表（默认限流防刷屏）。

## 命令总览

| 命令 | 作用 | DEB 数据源 | RPM 数据源 |
|---|---|---|---|
| `info` | 包详情 | dpkg status / apt | rpmdb / primary.xml |
| `list` | 文件列表 | dpkg info lists | rpmdb Header |
| `owns` | 文件归属 | dpkg -S 语义 / Contents | rpmdb 文件表 / filelists |
| `deps` / `rdeps` | 正/反向依赖 | Depends 关系 | Requires + 能力集 |
| `search` | 包名/摘要 + 文件路径 | apt 元数据 / Contents | primary / filelists |
| `source` | 源码包互查 | Source 字段 | Source RPM 字段 |
| `changelog` | 变更日志 | changelog.Debian | other.xml |
| `cache` | 缓存管理 | 多级缓存 | 多级缓存 |

## 缓存管理

```bash
pkq cache status           # 各缓存目录占用
pkq cache update           # 强制刷新仓库元数据（逐源进度 + 成败汇总）
pkq cache clean [target]   # 清理：all | index | repos | contents
```

## 平台支持

**仅支持 Linux**（依赖 `/var/lib/dpkg`、`/var/lib/rpm`、`/etc/os-release` 等
系统路径，不支持 Windows/macOS）。

- DEB 系：Deepin / UOS Desktop / Debian / Ubuntu
- RPM 系：UOS Server / openEuler / Fedora / CentOS 等

## 文档

**中文**：[命令参考](docs/zh_CN/命令参考.md) · [用户手册](docs/zh_CN/用户手册.md)

**English**: [Command Reference](docs/en/Command-Reference.md) · [User Manual](docs/en/User-Manual.md)

## License

[Apache-2.0](LICENSE)

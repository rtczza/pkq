# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.4] - 2026-09-14

### Fixed

- 安装器 PATH 持久化覆盖 SSH login shell（写 `.bash_profile` 等
  登录文件，此前仅写 `.bashrc` 导致新 SSH 会话找不到 `pkq`）

## [0.2.3] - 2026-09-14

### Added

- 安装脚本新增 `uninstall` 模式：`| sh -s -- uninstall` 一键卸载，
  删除二进制、安装器写入的 PATH 行与 `~/.cache/pkq` 缓存

## [0.2.2] - 2026-09-14

### Added

- 安装器将 PATH 持久化到 `~/.bashrc` / `~/.zshrc` / `~/.profile`
  （幂等，无 rc 文件时自动创建），新终端开箱即用，对齐 rustup 体验

### Fixed

- 解压改用 `tar -m`，消除时钟偏移主机上的未来时间戳警告

## [0.2.1] - 2026-09-14

### Added

- 一键安装：`curl ... | sh` 无写权限时自动降级 `~/.local/bin`，
  目标目录自动创建，安装后提示 PATH 配置
- README 新增 `cargo install pkq`（crates.io）安装说明

### Changed

- base64 改用标准 `base64` crate 替代手写实现，补充 RFC 4648 测试向量
- 缓存目录创建失败由静默忽略改为告警日志，便于排查权限问题

### Removed

- 删除易过时的中英文设计文档，文档收敛至命令参考与用户手册

## [0.2.0] - 2026-09-10

### Added

- 九大子命令：`info` `list` `owns` `deps` `rdeps` `search` `source` `changelog` `cache`
- DEB（dpkg status / apt 元数据 / Contents 索引）与 RPM（rpmdb 三格式原生解析 / repodata）双后端统一查询
- 语义对齐原生工具：`rdeps` 与 `dnf repoquery --whatrequires` 结果一致（强依赖 + 能力集匹配）
- 多级缓存：HTTP ETag/TTL → 解析结果 postcard → mmap Contents 检索；离线自动降级
- `--output json` 结构化输出（`--compact` 单行模式）与 `0/1/2`（命中/未找到/错误）退出码语义
- 运行时中英双语切换与 CJK 宽度对齐排版
- criterion 基准测试覆盖解析/缓存热路径（`cargo bench`）

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-09-10

### Added

- 九大子命令：`info` `list` `owns` `deps` `rdeps` `search` `source` `changelog` `cache`
- DEB（dpkg status / apt 元数据 / Contents 索引）与 RPM（rpmdb 三格式原生解析 / repodata）双后端统一查询
- 语义对齐原生工具：`rdeps` 与 `dnf repoquery --whatrequires` 结果一致（强依赖 + 能力集匹配）
- 多级缓存：HTTP ETag/TTL → 解析结果 postcard → mmap Contents 检索；离线自动降级
- `--output json` 结构化输出（`--compact` 单行模式）与 `0/1/2`（命中/未找到/错误）退出码语义
- 运行时中英双语切换与 CJK 宽度对齐排版
- criterion 基准测试覆盖解析/缓存热路径（`cargo bench`）

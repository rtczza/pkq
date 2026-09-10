# 贡献指南

感谢关注 pkq！提交贡献前请阅读以下规范。

## 开发环境

- Rust 1.89+（与 Cargo.toml 的 `rust-version` 一致；rustup 安装，建议 stable 工具链）
- Linux（DEB 或 RPM 系均可开发；双端功能需双环境验证）

## 提交前检查（全部通过才会被合并）

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## 双后端一致性

pkq 的核心契约是 **DEB 与 RPM 输出结构、字段对齐、交互体验 100% 一致**：

- 任何影响输出的改动，必须同时验证双端（可参考 `test_deb_all_cases.sh` /
  `test_rpm_all_cases.sh` 的断言模式）
- 涉及命令行为的改动请在 `CHANGELOG.md` 的 `[Unreleased]` 下记录

## 提交信息

- 格式：`<type>(<scope>): <描述>`，type 取
  `feat/fix/refactor/perf/test/docs/chore`
- 破坏性变更使用 `!` 并在正文说明（如 `refactor!: ...`）
- 一个提交聚焦一件事

## 报告问题

- 提交 Issue 时请附：发行版与版本（`cat /etc/os-release`）、
  包管理器版本、完整命令行、实际输出与期望输出
- 涉及仓库/源信息的场景请**脱敏**（替换内部镜像地址与凭据）

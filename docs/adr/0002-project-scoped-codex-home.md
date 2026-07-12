# ADR-0002：Project-scoped CODEX_HOME

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

全局或账号共享的 `CODEX_HOME` 会混合 Codex Session / Thread、配置、历史和缓存；将凭证与项目状态放在同一位置也会妨碍账号顺序切换。

## Decision

每个 Project 永久拥有独立的 Project Runtime，位于 `~/.local/share/muxlane/projects/<project-id>/codex-home`。`project_id` 由路径规范化信息的稳定 hash 派生，具体算法待阶段 2 POC。Account 只拥有独立 Account Vault。启动时将 Vault 的 `auth.json` 原子复制为 Runtime 活动凭证并设置 `CODEX_HOME`；退出或 Recovery 时再原子 Credential Commit 回 Vault 并清理 Runtime 活动凭证。切换 Account 不改变 Project Runtime。

## Consequences

- 同一 Project 可保留连续 Session、配置和历史，同时顺序使用本人拥有的不同 Account。
- Runtime 永不位于源码目录、`/mnt/c`、`/mnt/d`、OneDrive 或云同步目录。
- 凭证事务、Hash 冲突保留、fsync 与幂等 Recovery 必须在后续阶段实现；本 ADR 不定义其状态机。

## Alternatives

- **每 Account 一个完整 CODEX_HOME：** Account 切换会改变项目会话上下文。
- **全局共享 CODEX_HOME：** 无项目隔离且容易发生配置、缓存与 Session 串扰。
- **CODEX_HOME 放在源码仓库：** 存在 Git 扫描、同步、权限与跨平台文件语义风险。

## Security impact

Token 不进入 SQLite；Account Vault 目录要求 `0700`，`auth.json` 要求 `0600`。不得移动 Vault 原件，不得将 Prompt 或凭证写入遥测。

## Compatibility impact

Project Runtime 位于 WSL Linux 文件系统；源码目录可以在 Windows 或 WSL 文件系统。现有 Codex 文件格式只能通过受支持行为使用，不承诺解析或固定其内部 Schema。

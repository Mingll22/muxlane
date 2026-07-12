# Architecture Decision Records

ADR 记录影响 Muxlane 长期边界的架构决策。它们是设计依据，不代表相应能力已经实现。

## 规则

- 文件名使用四位递增编号和 kebab-case 标题，例如 `0001-example.md`。
- 状态使用 `Proposed`、`Accepted`、`Superseded` 或 `Deprecated`；ADR-0001～0012 已在阶段 1 设计冻结中接受。接受设计不等于业务能力已实现，后续 POC 推翻假设时必须以新的 ADR 替代，不能静默改写历史记录。
- 每份 ADR 必须包含：日期、状态、Context、Decision、Consequences、Alternatives、Security impact、Compatibility impact、Supersedes、Superseded by。
- 新决策不得重写历史 ADR；使用新的 ADR 替代并在两端互相链接。

## 索引

| ADR                                                          | 状态     | 标题                                       |
| ------------------------------------------------------------ | -------- | ------------------------------------------ |
| [0001](0001-windows-gui-wsl-control-plane.md)                | Accepted | Windows GUI 与 WSL Control Plane           |
| [0002](0002-project-scoped-codex-home.md)                    | Accepted | Project-scoped CODEX_HOME                  |
| [0003](0003-exclusive-project-and-account-locks.md)          | Accepted | 排他的 Project Lock 与 Account Lock        |
| [0004](0004-json-rpc-over-local-transport.md)                | Accepted | 本地传输上的版本化 JSON-RPC                |
| [0005](0005-atomic-credential-checkout-and-commit.md)        | Accepted | 原子 Credential Checkout 与 Commit         |
| [0006](0006-durable-launch-transaction-journal.md)           | Accepted | Durable Launch Transaction Journal         |
| [0007](0007-process-identity-with-boot-id-and-start-time.md) | Accepted | 使用 boot_id 与启动时间的进程身份          |
| [0008](0008-never-auto-resolve-credential-conflicts.md)      | Accepted | 绝不自动解决 Credential Conflict           |
| [0009](0009-sqlite-for-metadata-not-credentials.md)          | Accepted | SQLite for Metadata, Not Credentials       |
| [0010](0010-versioned-capability-negotiation.md)             | Accepted | Versioned Capability Negotiation           |
| [0011](0011-separate-control-and-terminal-data-planes.md)    | Accepted | Separate Control and Terminal Data Planes  |
| [0012](0012-forward-only-versioned-database-migrations.md)   | Accepted | Forward-only Versioned Database Migrations |

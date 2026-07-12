# Architecture Decision Records

ADR 记录影响 Muxlane 长期边界的架构决策。它们是设计依据，不代表相应能力已经实现。

## 规则

- 文件名使用四位递增编号和 kebab-case 标题，例如 `0001-example.md`。
- 状态使用 `Proposed`、`Accepted`、`Superseded` 或 `Deprecated`；阶段 1A 的记录为待阶段审查的 `Proposed` 冻结候选。
- 每份 ADR 必须包含：日期、状态、Context、Decision、Consequences、Alternatives、Security impact、Compatibility impact、Supersedes、Superseded by。
- 新决策不得重写历史 ADR；使用新的 ADR 替代并在两端互相链接。

## 索引

| ADR                                                 | 状态     | 标题                                |
| --------------------------------------------------- | -------- | ----------------------------------- |
| [0001](0001-windows-gui-wsl-control-plane.md)       | Proposed | Windows GUI 与 WSL Control Plane    |
| [0002](0002-project-scoped-codex-home.md)           | Proposed | Project-scoped CODEX_HOME           |
| [0003](0003-exclusive-project-and-account-locks.md) | Proposed | 排他的 Project Lock 与 Account Lock |
| [0004](0004-json-rpc-over-local-transport.md)       | Proposed | 本地传输上的版本化 JSON-RPC         |

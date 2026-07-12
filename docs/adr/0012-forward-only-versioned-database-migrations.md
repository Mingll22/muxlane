# ADR-0012：Forward-only Versioned Database Migrations

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

Muxlane 的 SQLite 只存非敏感 metadata，但它承载 Launch Transaction、Recovery 和兼容索引。Schema 变更若被旧 Daemon 写入、迁移中断或失败后静默重建，可能破坏审计和恢复。数据库与 Vault/Runtime 文件系统不是同一原子事务，也不能通过删除 DB 规避失败。

## Decision

数据库采用版本化、向前迁移。每次迁移在业务写操作之前获取迁移锁、做健康检查并创建受控备份；成功后持久化新的 `schema_version` 与 migration history。迁移失败进入诊断状态，保留 DB、备份和脱敏错误，不静默重建、不删除数据库。

已升级 Schema 的旧版本默认不得执行写操作。GUI/Daemon/CLI 先通过协议握手判断可用的只读诊断和升级要求；发布回滚边界是迁移前备份与失败诊断，不承诺任意版本无损降级。

## Consequences

- 实现阶段需要 migration lock、健康检查、备份、失败注入、版本兼容测试和清晰的 `MIGRATION_REQUIRED` 诊断。
- 迁移期间拒绝 Launch、凭证写入和其它业务写操作，防止部分 Schema 被并发使用。
- 新旧组件组合可能只允许只读诊断或要求升级，不能靠降级版本继续写入。

## Alternatives

- **静默删除/重建数据库：** 会删除恢复和审计证据，不能接受。
- **支持任意版本双向自动迁移：** 复杂且无法保证凭证/恢复相关历史无损，超出当前阶段证据。
- **不记录 schema version：** 无法安全判断升级、回滚和旧组件写入资格。
- **每次启动都复制新 DB：** 增加分叉和选择主事实来源的风险。

## Security impact

迁移失败保守阻止写操作，避免错误 Transaction/Recovery 决策。备份也包含 Sensitive local metadata，必须受控、脱敏且不能包含 Token 或完整凭证。

## Compatibility impact

该决策定义 forward-only 迁移原则而非 SQL 或最低 SQLite 版本。具体 schema、journal mode、备份位置、恢复命令和性能阈值必须经阶段 4/5 POC 验证。

# ADR-0009：SQLite for Metadata, Not Credentials

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

Muxlane 需要查询 Account/Project 元数据、Launch Transaction、RecoveryIncident、UsageSnapshot、关系和 schema version，同时还要在崩溃后解释文件系统副作用。将 `auth.json`、access token 或 refresh token 放进这些索引会扩大数据库、日志、备份、导出和迁移的泄露面。SQLite transaction 也不能替代 Linux `flock`，更不能使 SQLite commit 与 Vault/Runtime rename 成为一个原子操作。

## Decision

SQLite 只保存非敏感 metadata、关系、状态索引、durable transaction、Recovery 记录、Usage 缓存、非敏感设置和 migration history。Account Vault 是凭证的唯一主事实来源；完整 `auth.json`、access token、refresh token 和可恢复秘密不进入 SQLite。

SQLite 不是 Account Vault，也不是锁服务：活动互斥继续以 **Account Lock → Project Lock** 的真实 `flock` 为准。跨 SQLite 与文件系统的凭证操作先持久化意图、执行受控副作用、再持久化后继状态，并由 Recovery State Machine 幂等收口。

## Consequences

- 数据库可用于元数据查询、历史、恢复索引、迁移和可脱敏诊断，但凭证文件仍受 Vault/Runtime 权限与文件操作策略保护。
- 事务设计必须处理 SQLite/文件系统之间的中断窗口，不能把单个 DB transaction 当作凭证提交成功证据。
- 数据库备份、导出、损坏与迁移仍属 Sensitive local metadata 风险，需要脱敏和健康检查。

## Alternatives

- **所有状态只用 JSON 文件：** 多对象关系、并发索引、compare-and-set、迁移 history 和一致诊断会更难可靠实现。
- **凭证存入数据库 Blob：** 扩大备份、导出、迁移、查询和日志泄露面，且混淆 Vault 边界。
- **仅依赖内存状态：** GUI/Daemon/WSL 崩溃后无法恢复或审计。
- **每 Project 独立 Muxlane 数据库：** 跨 Project Account 独占、Recovery 和迁移协调更复杂，不能提供单一本机控制面视图。

## Security impact

Token、refresh token 与完整 `auth.json` 禁入 SQLite、普通日志、RPC 和普通诊断包。SQLite 记录的 hash、路径和错误摘要仍须最小化、脱敏和权限保护；锁与凭证完整性不因数据库存在而放松。

## Compatibility impact

SQLite schema 采用版本化向前迁移，具体 DDL/WAL 模式由 POC 和实现阶段验证。任何将 SQLite 当作凭证或 `flock` 替代品的未来方案必须通过新 ADR 替代本决策。

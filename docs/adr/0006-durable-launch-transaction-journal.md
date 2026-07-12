# ADR-0006：Durable Launch Transaction Journal

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

GUI 内存会在窗口、WebView 或 Windows 进程退出时消失，不能证明凭证是否已签出、Codex 是否启动或 Vault 是否已签回。单独 SQLite 占用字段同样不是 flock，不能表达文件操作、进程身份和多步恢复的因果关系。

## Decision

每次 Launch 建立一个 durable transaction，按 [Recovery State Machine](../RECOVERY_STATE_MACHINE.md) 持久化 preparing、checked_out、running、codex_exited、committing_auth、auth_committed、cleaned 与终态。事务关联 Project/Account、锁生命周期、凭证 Hash、受控备份引用和 boot_id/PID/start ticks/process identity；它不存储 Token。

事务不是锁的替代品：实际互斥由 ADR-0003 的 flock 提供；事务将文件系统、锁和进程证据连接起来，支持 daemon 启动或 CLI recover 的幂等决策。具体持久化数据模型、SQLite schema 与迁移策略仍将在阶段 1C 冻结。

## Consequences

- GUI 关闭或 Daemon 崩溃后仍可解释 Launch 的安全阶段，并避免将缺失内存状态当作正常退出。
- 每个有副作用的步骤增加事务写入、错误处理和故障注入要求。
- 恢复操作可重复，但每次对 Vault 的写入都要重新验证状态和 Hash。

## Alternatives

- **只保留 GUI 状态：** GUI 不在 WSL 且不是唯一恢复入口，崩溃后无证据。
- **只保留 SQLite 占用字段：** 不能替代内核锁，也无法表示凭证原子操作和 PID 身份。
- **只依赖日志：** 日志可能被截断、轮换或脱敏，且不是可执行状态机。

## Security impact

事务使凭证副本、Hash 冲突与进程身份可审计，而不保存 Token、Prompt 或原始终端数据。无事务或状态不完整时必须保守地进入 failed 或 credential_conflict，不得猜测成功。

## Compatibility impact

保持 SQLite 仅为元数据/状态索引而非排他真相。schema、保留期、并发实现和迁移 API 尚未实现，须与阶段 4 Recovery POC 一起验证。

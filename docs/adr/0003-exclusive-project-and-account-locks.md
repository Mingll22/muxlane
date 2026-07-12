# ADR-0003：排他的 Project Lock 与 Account Lock

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

同一 Account 并行使用会产生 OAuth refresh、凭证签回和副本冲突；同一 Project 的多个主实例会破坏 Runtime、Session 和终端的可恢复性。

## Decision

每次 Launch Transaction 必须同时取得 Project Lock 与 Account Lock。Linux `flock` 是主要排他机制；同一 Account 和同一 Project 均禁止并行，不同 Project 使用不同 Account 才可并行。固定获取顺序为 Account Lock 后 Project Lock，释放顺序相反。SQLite 只记录可见状态，不能代替锁。

## Consequences

- 冲突必须向用户说明占用对象和恢复建议，不得静默抢占、自动切号或仅按心跳超时认定锁失效。
- 锁记录需要关联 `boot_id`、PID、进程启动时间和事务 ID，以降低 PID 重用误判；内核锁的实际持有状态优先。
- WSL 重启后由 Recovery 依据事务与进程身份重新判定，不能相信陈旧数据库标记。

## Alternatives

- **仅 SQLite 占用字段：** 崩溃或并发写入不能提供可靠内核互斥。
- **仅 GUI 内存状态或心跳：** GUI 关闭、暂停或网络短暂异常会产生假阳性。
- **允许同一 Account 并行：** 无法安全处理 refresh token 和 Credential Commit 冲突。

## Security impact

排他锁防止 Token 副本竞争和跨 Project 串号。故障处理保留冲突副本供人工恢复，禁止覆盖未知较新凭证。

## Compatibility impact

依赖 WSL2 Linux 的 `flock` 语义。具体锁文件格式和错误码留给阶段 3/4 POC 与实现冻结。

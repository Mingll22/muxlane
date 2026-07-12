# ADR-0008：绝不自动解决 Credential Conflict

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

Codex 可能在 Runtime 中刷新凭证；与此同时 Vault 也可能被合法的后续操作更新。若恢复时 Vault 和 Runtime 都不同于 checkout 前 Hash，二者都可能有效。使用修改时间、最后写入者或 last-write-wins 无法证明哪个 refresh token/会话状态正确，自动选择可能永久丢失可用凭证。

## Decision

当 Vault 已变化且 Runtime 也变化时，事务进入 credential_conflict。实现必须保留当前 Vault 凭证、Runtime 遗留凭证、checkout 前备份或至少可审计 Hash，以及脱敏事务记录；禁止自动覆盖 Vault、自动删除 Runtime 或自动切换 Account。该状态阻断关联 Account/Project 的新 Launch，直到用户重新登录或通过人工处理生成明确审计结论。

Recovery 仅在 Vault 未变化时可签回更新的 Runtime，或在 Runtime 未变化而 Vault 已变化时安全清理遗留 Runtime；完整矩阵由 [Recovery State Machine](../RECOVERY_STATE_MACHINE.md#7-hash-冲突决策矩阵) 定义。

## Consequences

- 可避免在异常恢复中静默丢失 Token 刷新结果或串号。
- 用户遇到冲突时必须手工处理，恢复流程不追求“无提示自动成功”。
- 冲突副本的权限、加密（若未来引入）和保留期需作为敏感资产治理，并保证诊断包不包含内容。

## Alternatives

- **last-write-wins：** 写入顺序不代表凭证有效性，可能覆盖较新的合法 Vault。
- **总是 Runtime 优先：** Daemon 崩溃或旧 Runtime 可能覆盖之后的登录。
- **总是 Vault 优先并删除 Runtime：** 可能丢失 Codex 运行期间的 refresh。
- **自动切换其他 Account：** 违反非账号池、非自动切号和显式用户选择原则。

## Security impact

冲突保留把可用性成本置于凭证完整性之前，防止自动覆盖。恢复、日志和导出必须仅暴露状态/Hash 结果，永不暴露凭证内容。

## Compatibility impact

不假定 auth.json 的内部字段或 Token 可比较，只依赖安全文件操作与内容 Hash。重新登录的 UX、Vault 备份格式和人工处理命令将在后续阶段设计并经 POC 验证。

# ADR-0005：原子 Credential Checkout 与 Commit

- 状态：Accepted
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

Codex 在运行期间可能刷新 Runtime 中的 auth.json。移动 Account Vault 原件会在启动失败、进程崩溃或跨文件系统操作时制造唯一凭证丢失窗口；直接写目标文件会让读者看见部分内容，也无法可靠处理断电后的目录项持久化。

## Decision

Vault auth.json 永远是长期凭证副本，Checkout 只能复制到 Project Runtime；Commit 只能在 Codex 已确认退出且双锁仍持有时将 Runtime 副本签回 Vault。每次敏感写入使用：受控同目录临时文件、先设为 0600、完整写入和验证、文件 fsync、同目录原子 rename、父目录 fsync。实现必须验证常规文件类型、受控父目录和非预期符号链接，计算 Hash，并保留足以处理冲突的副本。

敏感临时文件必须在目标同目录，不能跨文件系统假设 rename 原子；Linux rename 跨不同挂载点会失败，文件 fsync 本身也不保证包含它的目录项已经持久化。[rename(2)](https://man7.org/linux/man-pages/man2/rename.2.html) [fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html)

## Consequences

- Runtime auth.json 只在活动 Launch Transaction 中存在，Codex 更新可在退出时安全签回。
- 每个关键步骤必须由 durable transaction 记录，且失败后通过 Hash 矩阵恢复；实现复杂度、I/O 和故障注入测试增加。
- 凭证副本、临时文件和备份必须受同等权限与脱敏日志策略保护。

## Alternatives

- **移动 Vault 原文件：** 启动中断或移动失败时可能没有可信凭证副本。
- **直接覆盖目标：** 读者可能看见部分文件，断电恢复与证据保留不足。
- **跨目录或跨文件系统临时文件：** 不能保证 rename 原子，且可能引入 EXDEV 和权限边界变化。
- **不签回 Runtime：** 丢失 Codex 运行中刷新后的长期凭证状态。

## Security impact

不移动 Vault 原件减少单点丢失；同目录原子替换、权限、Hash 和 no-follow 路径策略降低泄露、TOCTOU 与部分写入风险。Hash 冲突不允许自动覆盖，具体处置由 ADR-0008 和恢复状态机定义。

## Compatibility impact

依赖 WSL Linux 文件系统的文件权限、fsync 和同目录 rename 语义。实际文件 API、异常分类和 Codex auth.json 格式/刷新行为必须在阶段 3–4 POC 验证；本 ADR 不实现或冻结它们。

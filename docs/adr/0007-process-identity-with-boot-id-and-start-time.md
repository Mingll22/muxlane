# ADR-0007：使用 boot_id 与启动时间的进程身份

- 状态：Accepted
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

PID 会重用；WSL/Linux 重启后旧 PID 更不具备任何身份含义。只凭 tmux Session、心跳或 SQLite 状态也可能将无关进程识别为受管 Codex，进而误杀进程或签回仍在被修改的凭证。

## Decision

每个受管 Runner/Codex 的事务记录 Linux boot_id、PID、/proc/pid/stat 的 start ticks 与可验证的 cmdline/process identity 摘要。恢复判断顺序固定为：真实 flock 状态、boot_id、PID、start ticks、cmdline/process identity、heartbeat。单独 PID 不可靠；boot_id 改变表示不能将旧 PID 解释为同一 Linux 实例；heartbeat 仅用于展示。

无法确认身份时，Muxlane 禁止 kill、附加为受管进程、自动签回凭证或声明已恢复，必须进入人工处理的 failed 状态。Linux 内核将 boot_id 描述为启动后不变的 UUID，/proc/pid/stat 字段 22 是进程自启动后的 starttime ticks。[Linux kernel documentation](https://docs.kernel.org/admin-guide/sysctl/kernel.html) [proc_pid_stat(5)](https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html)

## Consequences

- 降低 PID 重用和 WSL 重启导致的误杀、错误附加和凭证串号风险。
- 恢复实现需要可靠读取 /proc、处理访问/解析失败，并避免将敏感完整 cmdline 写入日志。
- 心跳仍可用于 UX，但不再是锁失效或进程死亡的判断依据。

## Alternatives

- **仅 PID：** PID 重用与重启后必然不可靠。
- **仅 tmux Session：** Session 存在不证明其中 Codex 存活，名称也可能冲突。
- **仅心跳或 GUI 内存：** 断线、暂停和 GUI 崩溃会产生假死亡。

## Security impact

保守的未知身份处理优先避免杀错进程和覆盖仍可能更新的 Runtime 凭证。它不能防护已完全失陷的同 UID 环境，因此还需要受控路径、Socket 权限与双锁。

## Compatibility impact

依赖目标 WSL Linux 的 /proc 格式与 boot_id 可用性。实际 Runner/Codex 进程树、cmdline 签名和受限 /proc 可见性必须在阶段 3–4 POC 验证。

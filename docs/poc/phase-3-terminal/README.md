# 阶段 3 Terminal POC

> 结论：**BLOCKED**（截至本提交）。tmux Control Mode 与 WSL 端 Gateway 已有真实本地证据；Windows Tauri Host、真实 xterm.js GUI 生命周期和无损 history/live 拼接尚未验证。此目录不描述正式 Runtime、Daemon、账号、凭证或恢复能力。

## 目标

验证一个明确标为非生产的终端链路：Tauri Host 的有限 Command → 固定 `wsl.exe --exec muxlaned phase3 gateway` → 专用 tmux socket 的 Control Mode → synthetic 终端任务。所有测试 Session 以 `mlp3-` 开头，且只使用独立 socket。

## 非目标

- Phase 4 的锁、事务、Crash Recovery 或重启恢复；
- Phase 5 的生产 Daemon、Repository、数据库、RPC 或进程监督；
- Phase 6/7 的账号、项目、额度、文件或工作台功能；
- Codex 登录、`auth.json`、真实 prompt、用户 Shell 或用户 tmux Server。

## 目录

- [环境与探测](ENVIRONMENT.md)
- [设计与数据面](DESIGN.md)
- [测试计划](TEST_PLAN.md)
- [脱敏结果](RESULTS.md)
- [安全审计](SECURITY.md)
- [限制](LIMITATIONS.md)

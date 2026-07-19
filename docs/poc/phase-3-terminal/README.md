# 阶段 3 Terminal POC

> 结论：**PASS WITH LIMITATION**（2026-07-19）。history/live bootstrap、连接隔离、真实 Windows Tauri 链路、GUI 生命周期、双 Project/双 Window 与安全边界均已通过；保留的限制仅涉及 POC 安装发现、production-grade 调度和既有依赖告警。此目录不描述正式 Runtime、Daemon、账号、凭证或跨系统重启恢复能力。

## 已验证目标

验证一个明确标为非生产的终端链路：xterm.js → Tauri Host 的有限 Command → 固定 `wsl.exe --exec /usr/bin/env muxlaned phase3 gateway` → 专用 tmux socket 的 Control Mode → synthetic 终端任务。

- history 只在 attach/recover 时执行一次有限 `capture-pane`；Control Mode pause/barrier 保证 snapshot 与 live 之间没有静默窗口；
- connection、attachment、bootstrap、Project、Window、Pane 和 xterm 实例均具有可核验身份；
- 每条事件具有单调 sequence；旧 stream、跨 Pane 事件和序号 gap 均被拒绝；
- 控制队列同时限制为 1024 行和 2 MiB，单行 256 KiB、单事件 64 KiB，溢出显式关闭当前流；
- Windows release Tauri 应用不创建 TCP listener；测试期 Vite/CDP 只绑定 loopback。

## 非目标

- Phase 4 的锁、事务、Crash Recovery 或 Windows/WSL 重启恢复；
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

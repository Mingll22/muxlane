# 环境与 Control Mode 探测

最终验证日期：2026-07-19。以下是脱敏摘要；原始 Home、主机、IP、欢迎信息和用户路径均未保存。

| 项目         | 结果                                                                  |
| ------------ | --------------------------------------------------------------------- |
| Windows Host | Windows x64；Tauri dev/release 原生窗口 PASS                          |
| WSL          | Ubuntu WSL2；未执行 `wsl --shutdown`                                  |
| Rust         | 1.97.0；Windows MSVC Workspace check/clippy/test PASS                 |
| Node / pnpm  | Windows/WSL Node 22.22.2 / pnpm 10.16.1                               |
| tmux         | 3.4                                                                   |
| Control Mode | PASS：typed command barrier、raw-byte `%output`、pause/resume、detach |
| 专用 socket  | PASS：只使用 `muxlane-p3` 或每测试唯一 socket，不访问默认 server      |
| WebView2     | Windows 原生 Tauri dev/release 启动 PASS                              |

## 观察到的事件语义

Control Mode 返回 `%begin` / `%end` 命令边界、`%session-changed`、`%output <pane> <octal-escaped-bytes>`、Window/layout/close 事件。`%output` 按原始字节解析，不要求每个控制行中的 payload 独立构成 UTF-8。

一次性探针确认 `refresh-client -A '<pane>:off'` 会冻结目标 Pane 的 Control Mode output，`:on` 恢复并按序交付冻结期间输出；`pause` 子命令不提供所需语义。此结果用于 bootstrap barrier，探针 server 已清理。

tmux 显示 ID 为 Session `$N`、Window `@N`、Pane `%N`。POC 只接受目标受管 Session 真实枚举出的 `@<digits>` / `%<digits>`，不接受 WebView 任意 target。

## Windows 验证方式

Windows checkout 使用同一 Git commit；没有把文件从 WSL 工作区复制回 Windows。调试运行通过固定 Host 启动 WSL Gateway，WebView2 CDP 只用于自动化真实 Tauri 窗口；release 构建随后在无 Vite/CDP 的条件下启动并完成 listener 审计。

WSL 默认非交互 PATH 中未安装系统级 `muxlaned`，验证时只把当前构建目录加入启动进程 PATH。`/usr/bin/env muxlaned` 可解析该受控二进制，而直接 `wsl.exe --exec muxlaned` 不执行 PATH 查找；因此 Host 使用固定 `/usr/bin/env` argv。

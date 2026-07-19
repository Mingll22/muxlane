# 阶段 3 测试计划与状态

自动化 tmux 测试只创建专用 `tmux -L muxlane-p3-<pid>-<case>` socket，并由 harness 在 drop 时 `kill-server`。Windows 原生构建不运行 Linux/WSL-only tmux 集成目标；同一目标在 WSL 真实执行。

| 项目                                             | 证据层                                              | 状态 |
| ------------------------------------------------ | --------------------------------------------------- | ---- |
| Control Mode probe、原始字节 output parsing      | Rust 单元 + WSL tmux 3.4                            | PASS |
| bootstrap/history/live 拼接与旧 stream 拒绝      | WSL 集成，3 次 reconnect                            | PASS |
| 连续大输出与有界队列                             | 512 条 burst + overflow 单元                        | PASS |
| ID、target、尺寸、空/超大输入拒绝                | Rust 单元/集成                                      | PASS |
| xterm 单实例、listener 生命周期、sequence cursor | Vitest                                              | PASS |
| Windows xterm→Tauri→WSL→Gateway→tmux             | 原生 Tauri dev + release                            | PASS |
| Enter、Backspace、Tab、方向键、ANSI              | WebView CDP 输入 + Pane 字节/渲染证据               | PASS |
| CJK、Emoji、宽字符                               | WebView 输入、xterm 渲染、Pane hex                  | PASS |
| Ctrl+C                                           | 目标 auxiliary 前台进程退出，其他 Pane/Gateway 存活 | PASS |
| resize                                           | 实际 Tauri 窗口 resize + tmux pane size             | PASS |
| GUI close / force kill / reopen                  | Windows Tauri，多轮关闭与重启                       | PASS |
| Session/Window/history/live 恢复                 | Windows Tauri + tmux                                | PASS |
| 2 Project × 2 Window 隔离                        | Windows Tauri + 四个真实 Pane                       | PASS |
| 重复 attach/detach/reconnect                     | Rust 集成 + GUI 3 次 reconnect                      | PASS |
| Capability/CSP、持久化、listener                 | source + release runtime audit                      | PASS |

## Windows GUI 可重复验收边界

- 工具链：Windows Node 22、pnpm、Rust MSVC、Build Tools、Windows SDK、WebView2；
- WSL PATH 仅加入当前验证构建的 `muxlaned` 目录；Host 仍只能启动编译期固定 argv；
- synthetic 数据只含 project/window 标签、递增序列、ANSI、中文、宽字符和 Emoji；
- 调试运行使用 loopback Vite `127.0.0.1:1420` 和临时 WebView2 CDP `127.0.0.1:9333`；release 无 listener；
- 不执行 `wsl --shutdown`，不访问默认 tmux socket，不读取账号或凭证。

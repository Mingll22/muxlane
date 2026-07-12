# 阶段 3 测试计划与状态

所有自动化 tmux 测试应创建专用 `tmux -L muxlane-p3-...` socket，并在 `finally`/trap 中 `kill-server`。不使用默认 server。

| 项目 | 自动化 | E2E | 当前状态 |
| --- | --- | --- |
| Control Mode probe / output parsing | 单元 + 手工隔离 probe | WSL | PASS |
| 创建、列表、输入、resize、detach、cleanup | Gateway JSON-lines | WSL | PASS（单 Project/Window） |
| target / ID / frame / resize 注入拒绝 | Rust 单元 | N/A | PASS |
| xterm 生命周期 | 前端 build/test | Tauri | NOT VERIFIED |
| ANSI、CJK、Emoji、宽字符 | Gateway synthetic 输出 | xterm GUI | WSL PASS；GUI NOT VERIFIED |
| Ctrl+C | N/A | Tauri + WSL | NOT VERIFIED |
| history + realtime 无丢失拼接 | N/A | Tauri + WSL | NOT VERIFIED |
| GUI close / kill / reopen | N/A | Windows Tauri | BLOCKED |
| 多 Project / 多 Window 隔离 | N/A | Tauri + WSL | NOT RUN |
| 大输出、背压 | N/A | Tauri + WSL | NOT RUN |
| Capability/CSP Command audit | source audit | Tauri | source PASS；runtime NOT VERIFIED |

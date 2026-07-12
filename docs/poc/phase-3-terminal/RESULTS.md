# 脱敏执行结果

## 已执行

| 命令/动作                                                                                           | 退出码 | 结果                                       |
| --------------------------------------------------------------------------------------------------- | -----: | ------------------------------------------ |
| `tmux -V` 与独立 `tmux -L ... -C` 探测                                                              |      0 | PASS，tmux 3.4、Control Mode 事件已观察    |
| `cargo test -p muxlaned -p muxlane-protocol`                                                        |      0 | PASS，4 tests passed、0 failed、0 ignored  |
| `cargo clippy -p muxlaned -p muxlane-protocol --all-targets --all-features -- -D warnings`          |      0 | PASS                                       |
| `pnpm --filter muxlane-desktop typecheck`                                                           |      0 | PASS                                       |
| `pnpm --filter muxlane-desktop test`                                                                |      0 | PASS，1 test passed、0 failed、0 skipped   |
| `pnpm --filter muxlane-desktop build`                                                               |      0 | PASS；Vite 报告单 chunk 大于 500 kB 警告   |
| Gateway synthetic Session → history → Control Mode output → bytes input → resize → detach → cleanup |      0 | PASS，专用 tmux server 已清理              |
| `pnpm audit --prod --audit-level=high`                                                              |      0 | PASS，未发现已知 high 级生产依赖漏洞       |
| `cargo audit`                                                                                       |      0 | PASS（漏洞退出码）；17 条 advisory warning |
| 双 Project、双 Window Gateway 集成测试                                                              |      0 | PASS，独立 ID、切换、resize、拒绝与清理    |

Gateway 真实输出含 ANSI 转义、`中文`、`表` 与 Emoji；控制帧中的数据作为字节数组观察，未写入原始终端日志。

## 环境阻塞

| 命令                                                                                           | 退出码 | 原因                                                           |
| ---------------------------------------------------------------------------------------------- | -----: | -------------------------------------------------------------- |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`                                |      1 | WSL 缺少 `pkg-config` 与 GTK/GLib 开发依赖                     |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-gnu` |    101 | 交叉 Windows resource 工具 `x86_64-w64-mingw32-windres` 不存在 |

真实 Windows Tauri GUI、窗口关闭、GUI 进程终止、重开、xterm 连接 tmux、Ctrl+C、隔离和 CI：**NOT RUN / NOT VERIFIED**。没有以 Vite 浏览器构建替代它们。

当前结论为 **BLOCKED**，不能创建 PR、push 或合并。

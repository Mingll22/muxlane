# 脱敏执行结果

验证日期：2026-07-19。所有终端数据均为 synthetic；未保存原始主机日志、用户路径、账号、Token、Cookie、prompt 或业务输出。

## 自动化与构建

| 命令/动作                                                                  | 平台            | 退出码 | 结果                                                |
| -------------------------------------------------------------------------- | --------------- | -----: | --------------------------------------------------- |
| `pnpm verify`                                                              | WSL             |      0 | PASS；前端 3 files / 5 tests，Rust WSL 集成 5 tests |
| `pnpm verify`                                                              | Windows Node 22 |      0 | PASS；WSL-only tmux test target 明确为 0 tests      |
| `cargo fmt --all -- --check`                                               | WSL/Windows     |      0 | PASS                                                |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings`     | Windows MSVC    |      0 | PASS，含 Desktop                                    |
| `cargo test --workspace --all-features`                                    | Windows MSVC    |      0 | PASS，含 Desktop；tmux 集成只在 Linux 执行          |
| `cargo test -p muxlaned -p muxlane-protocol --all-features -- --nocapture` | WSL             |      0 | PASS；1 protocol + 5 unit + 5 tmux integration      |
| Windows Desktop `cargo check --all-targets`                                | Windows MSVC    |      0 | PASS                                                |
| `pnpm --filter muxlane-desktop tauri build --no-bundle`                    | Windows         |      0 | PASS；生成 release `Muxlane.exe`                    |
| `pnpm audit --prod --audit-level=high`                                     | WSL             |      0 | PASS；无已知漏洞                                    |
| `cargo audit`                                                              | WSL             |      0 | 无可阻断 vulnerability；17 个 allowed warnings      |

Vite production bundle 保留单 chunk 大于 500 kB 警告。WSL 缺 GTK/GLib 开发包，因此 Desktop Rust 的完整原生验证由 Windows MSVC 完成，不用交叉编译替代。

## sequence 与数据面

- bootstrap/reconnect 共 3 个新 stream，最终复验分别观察 23、35、47 个连续 tick，总计 105；missing=0、duplicate=0、reordered=0、cross_stream=0；
- 每个 stream 的 event sequence 都从 0 连续增长；旧 stream 输入返回 `stale_stream`；
- 512 条 burst：missing=0、duplicate=0、reordered=0、overflow=0；
- 包含 bootstrap 失败清理的 5-test tmux 集成套件连续运行 5 次，全部通过；
- 原始 UTF-8 高字节拆帧单元覆盖 `0xe4`，避免 Control Mode 行解析隐式要求 UTF-8。

## Windows 原生 Tauri 实链

真实链路为：

```text
xterm.js → Tauri Host → wsl.exe → muxlaned Phase 3 Gateway → tmux Control Mode → synthetic runner
```

- ASCII 编辑序列经 Backspace 后，Pane 收到目标文本；Tab 为 `09`、ArrowUp 为 `1b5b41`、Enter 经 PTY 为 `0a`；
- `中文表😀` 的 Pane 证据为 `e4b8ade69687e8a1a8f09f98800a`；
- xterm DOM 同时观察到 ANSI cyan 与默认前景色，连续 live output 正常；
- Tauri 窗口从 `1200×800` 调整到 `900×600` 时，目标 `project-a/@0` 从 `115×29` 变为 `76×19`，非目标 `project-b/@2` 保持 `225×48`；
- Ctrl+C 关闭目标 `project-a/@1` synthetic 前台进程并产生 `stream_closed`；`@0`、`project-b/@2`、Gateway 和 Tauri 进程继续运行，随后 `@0` 仍可接收输入；
- 正常 close、强制终止 GUI 进程、再次 close 后 tmux Session 均保留；未执行 `wsl --shutdown`，Docker 容器 ID 与健康状态保持；
- 重开自动发现 `mlp3-project-a`/`mlp3-project-b` 并恢复 `project-a/@0`；一次恢复先收到 `history / 21535 bytes`，历史中存在关闭前 marker，随后恢复 live output；
- GUI 重复 reconnect 3 次，每次只观察到一个新的 Control Mode client，无 listener/xterm 重复注册症状。

## 多 Project / 多 Window

建立过以下四个真实 target：

```text
mlp3-project-a @0 muxlaned %0
mlp3-project-a @1 aux      %1
mlp3-project-b @2 muxlaned %2
mlp3-project-b @3 aux      %3
```

四个输入 marker 只出现在各自 Pane；同名 `aux` 依靠内部 ID 区分。关闭 `project-b/@3` 后，`@2` tick 从 2168 继续到 2262。切换、detach、reconnect 和恢复均未观察到跨 Project/Window 输出或旧事件进入当前标签。

## 结论

核心 Phase 3 门禁为 PASS。总体结论为 **PASS WITH LIMITATION**，仅保留 [LIMITATIONS.md](LIMITATIONS.md) 中的非生产限制；可以进入 PR/CI 阶段，但这些 POC 结果不能表述为正式生产实现。

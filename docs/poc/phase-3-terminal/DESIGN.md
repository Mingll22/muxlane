# POC 设计

## 明确的非生产边界

`muxlaned phase3 gateway` 是 stdio POC 子命令，不是常驻生产 daemon。它不监听网络或 Unix socket，不读写凭证、数据库、项目目录或 `CODEX_HOME`，也不接受任意可执行程序、shell 字符串或 tmux target。

## 固定桥接

Windows Tauri Host 的唯一子进程形状固定为：

```text
wsl.exe --exec /usr/bin/env muxlaned phase3 gateway --socket muxlane-p3
```

`/usr/bin/env`、`muxlaned`、子命令和 socket 都是编译期常量；使用 `env` 是因为 `wsl.exe --exec muxlaned` 不执行 PATH 查找，而 WSL 非交互环境可由部署步骤提供受控 PATH。WebView 不能传 executable、argv 或 shell 文本。

Host 通过带 request ID 的 typed JSON-lines 与 Gateway 通信，并把 typed terminal event 转发为单一 `phase3-terminal-frame` Tauri 事件。不存在 `execute(command)`、文件 API 或网络 API。

## 标识与隔离

每次 Gateway 启动生成 `connection_id`；每次 attach 生成 `attachment_id` 与 `bootstrap_id`。stream 同时携带 `project_id`、`window_id` 和 `pane_id`。输入、resize、detach 和 start-stream 必须回传完整 stream token；任何旧 connection/attachment/bootstrap 或错误 target 返回 `stale_stream` 或 validation error。

Project ID、socket 和 Window name 只接受长度受限的 `[a-z0-9-]` slug。Session 始终是 `mlp3-<project>`；Window/Pane 必须来自 Gateway 对目标 Session 的真实枚举，并分别匹配 `@<digits>` / `%<digits>`。输入以 `send-keys -H <hex-byte>` 写入，用户字节不会成为 tmux 命令文本。

前端在组件生命周期内只创建一个 xterm、一个 FitAddon、一个 ResizeObserver、一个 input subscription 和一个 Tauri listener。切换 stream 时 reset xterm、重建 UTF-8 decoder 和 sequence cursor；listener 在 `StartStream` 前就绪，旧事件直接丢弃，gap 会使当前流失效。

## history/live bootstrap

1. attach 先启动目标 Pane 的 tmux Control Mode client，并验证该 Pane 只有一个受管 client；
2. `refresh-client -A '<pane>:off'` 暂停目标 Pane 的 Control Mode输出，并等待 tmux `%begin/%end` command barrier；
3. 对仍稳定的目标执行一次 `capture-pane -p -e -J -S -300`；
4. Host 返回 stream token，前端确认 listener 已注册后显式发送 `StartStream`；
5. Gateway 以 sequence 0 发 history，启动 live forwarder，再用 `refresh-client ...:on` 恢复输出；
6. 5 秒内未 start 的 bootstrap 自动过期并恢复 Pane，避免长期冻结。

tmux 的 `off/on` pause 行为已在一次性隔离 server 实测；没有持续 capture-pane 轮询。Control Mode 按字节解析，避免拆分 UTF-8 多字节字符时把真实终端数据当成无效文本。

## 有界数据面

- 输入：`1..=16384` bytes；
- resize：columns `20..=320`、rows `5..=160`；
- Host pending request：32；Gateway control frame：128 KiB；
- Control Mode queue：1024 行且总计不超过 2 MiB；单行 256 KiB；单 output event 64 KiB。

超过任何界限都会显式拒绝或产生 stream error，不静默丢弃。POC 不承诺多租户公平调度或 production-grade metrics。

## 生命周期

detach、窗口关闭或 Host 退出只结束 Control Mode client，不调用 `kill-session`。关闭受管 Window 和 cleanup 必须是显式 typed operation。GUI 重开后枚举 `mlp3-` Session、恢复 active Window、重新执行有限 bootstrap 并继续 live；Windows/WSL 重启恢复不属于 Phase 3。

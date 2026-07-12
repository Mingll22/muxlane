# POC 设计

## 明确的非生产边界

`muxlaned phase3 gateway` 是 stdio POC 子命令，而不是常驻生产 daemon。它不监听 TCP/UDP/Unix socket，不读写凭证、数据库、项目目录或 `CODEX_HOME`，也不接受任意可执行程序、shell 字符串或 tmux 参数。

## 桥接

Windows Tauri Host 的唯一子进程形状固定为：

```text
wsl.exe --exec muxlaned phase3 gateway --socket muxlane-p3
```

WebView 只可调用命名的 Tauri Command。Host 用 JSON-lines 向 stdio Gateway 传递带请求 ID 的 typed request；读取到的 typed response 与 terminal event 被分别关联或转发为 `phase3-terminal-frame`。该物理 stdio 通道逻辑上分为：

| Plane   | 有限操作                                                                         |
| ------- | -------------------------------------------------------------------------------- |
| Control | probe、创建/列表 Session、创建/列表/关闭 Window、attach、detach、resize、cleanup |
| Data    | 有界输入字节帧、history 字节帧、Control Mode `%output` 字节帧、stream 状态       |

不存在 `execute(command)`、`run_shell(args)`、文件 API 或网络 API。

## tmux 与标识

Project ID、socket 和 Window name 只接受以小写 ASCII 字母开头、长度受限的 `[a-z0-9-]` slug。Session name 始终由 Gateway 构造为 `mlp3-<project>`。Window ID 必须是从该 Session `list-windows` 得到并验证的 `@<digits>`；target 仅由已验证的 project 和 ID 构造。输入以 `send-keys -H <hex-byte>` 写入 Control Mode，用户字节不会成为 tmux 命令文本。

列、行分别限制为 `20..=320`、`5..=160`；输入帧限制为 `1..=16384` 字节；Host pending control requests 限制为 32。Gateway 控制帧限制为 128 KiB。当前 POC 未实现完整慢消费者背压算法：stdout pipe 的阻塞是天然上游背压，缺少明确的 per-client 丢弃指标，见限制。

## 生命周期

Gateway 的 detach/Host drop 只结束 Control Mode client，不调用 `kill-session`；tmux Session 与 synthetic runner 理应独立于 GUI 存活。cleanup 是显式操作。Host 进程不会把请求参数拼接为 Windows 或 WSL shell 命令。

## history 与实时输出

首次 attach 使用一次 `capture-pane -p -e -J -S -300` 生成有界 history，再启动 Control Mode 实时 `%output`。这不是 capture-pane 轮询。但本 POC 尚未实现可证明的原子 bootstrap barrier：在 snapshot 与 Control Mode attach 间存在理论竞态，可能重复或遗漏极短输出。不得将当前实现表述为无损恢复；这是阶段核心阻断项。

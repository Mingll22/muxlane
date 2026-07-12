# ADR-0011：Separate Control and Terminal Data Planes

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

控制请求需要版本协商、请求/响应、错误分类和有限状态事件；Terminal 需要高频 PTY 字节、背压、有限历史、attach/detach 和连接中断后的恢复。把终端输出无限塞入普通 JSON-RPC notification 会让慢 Client 失控占用内存，也模糊 GUI 断开、tmux 存活和 Codex 退出之间的边界。

## Decision

Muxlane 逻辑上分离版本化 JSON-RPC Control Plane 与 Terminal Data Plane。`terminal.attach`、`detach`、`resize`、关闭资格和授权属于 Control Plane；PTY 输出、输入传输、背压和 history 恢复属于 Terminal Data Plane。终端连接丢失不等于 tmux 或 Codex 退出，重连从受管 tmux 的有界 history 恢复。

具体 Windows—WSL Bridge、WebSocket/本地桥、二进制帧、压缩、复用和 backpressure 算法均为阶段 3/5 **POC validation required**，不在本 ADR 冻结。

## Consequences

- 终端数据面必须支持 attach/detach、慢消费者限制和安全的输入授权；stdin 不可自动重放。
- Control Plane 事件只传 Terminal metadata，不传无限 stdout/stderr。
- tmux history 是终端恢复载体，而 Launch Transaction、`flock` 与进程身份仍分别决定恢复/运行事实。

## Alternatives

- **所有流量都走普通 JSON-RPC notification：** 无法独立处理背压、吞吐、重放和有限缓冲。
- **GUI 直接连接 tmux Socket：** 绕过 Tauri/Daemon 授权，扩大同用户终端控制面。
- **每次重连无限 capture-pane 轮询：** 性能、准确性和内存边界不足，且不能定义实时流契约。

## Security impact

只有已授权的 attachment 可输入；终端转义序列、OSC 52 和剪贴板能力需要明确安全策略。Control RPC、事件和错误不传终端原文、Token 或完整命令行。

## Compatibility impact

逻辑分层对 Client 稳定，实际帧格式可在 POC 后版本化演进。任何新增终端载体都必须保持连接断开不影响 Launch Transaction 的不变量。

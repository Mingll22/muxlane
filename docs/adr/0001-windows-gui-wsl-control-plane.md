# ADR-0001：Windows GUI 与 WSL Control Plane

- 状态：Accepted
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

目标用户在 Windows 10/11 上使用 WSL2 和 Codex CLI。GUI 需要在关闭或重启后仍可恢复受管任务，而 Account Vault、锁、事务和 `tmux` 需要 Linux 文件权限与进程语义。

## Decision

`Muxlane.exe` 在 Windows 侧负责交互，WSL 默认发行版中的统一 `muxlaned` 负责 Project Runtime、敏感操作和进程监督。GUI 的生命周期不得决定 Daemon、`tmux` 或 Codex 的生命周期；WSL CLI `muxlane` 是独立恢复入口。

## Consequences

- Windows GUI 保持薄客户端，不能直接读取 Account Vault 或任意执行 Shell。
- WSL 侧成为状态、锁和凭证事务的唯一权威；GUI 重连必须可恢复展示。
- 阶段 3 POC 需验证 Windows 到 WSL 的受控本地桥接、身份绑定和错误恢复。

## Alternatives

- **全部运行在 Windows：** 失去 Linux `flock`、`tmux` 和目标 CLI 运行环境的一致性。
- **每个 Project 一个 Daemon：** 提高锁协调、恢复和升级复杂度，且不利于统一诊断。
- **GUI 直接调用 WSL Shell：** WebView 到 Shell 的能力边界过宽，生命周期也会耦合。

## Security impact

缩小 WebView 权限并将 Account Vault 留在 Linux 用户目录。Tauri Host 仅暴露经过 Capability/ACL 约束的白名单桥接命令；不开放 LAN 监听。

## Compatibility impact

MVP 仅面向 Windows 10/11、WSL2、默认发行版和 Windows x64；不承诺多发行版、macOS、原生 Linux GUI 或 Windows ARM。

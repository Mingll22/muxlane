# Muxlane 架构入口

Muxlane 已完成阶段 1 设计冻结、阶段 2/3 POC、Phase 4/5 正式 WSL Runtime Control Plane，以及 Phase 6 Windows GUI 和重新划定范围后的 Phase 7 开发工作台。阶段关闭仍以 PR/CI 和 `main` 合并事实为准。

正式总体架构见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，需求范围见 [docs/PRD.md](docs/PRD.md)，运行安全与恢复冻结见 [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)、[docs/RUNTIME_LIFECYCLE.md](docs/RUNTIME_LIFECYCLE.md) 和 [docs/RECOVERY_STATE_MACHINE.md](docs/RECOVERY_STATE_MACHINE.md)，逻辑协议/数据模型/兼容策略见 [docs/PROTOCOL.md](docs/PROTOCOL.md)、[docs/DATA_MODEL.md](docs/DATA_MODEL.md) 和 [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md)，长期决策见 [docs/adr/README.md](docs/adr/README.md)。

## 架构摘要

- `Muxlane.exe` 是 Windows GUI；WSL 默认发行版内的单一 `muxlaned` 是 Runtime Control Plane。
- 每个 Project 设计为拥有永久的 Project Runtime 和 Project-scoped `CODEX_HOME`；Account 设计为独立 Account Vault。
- Launch Transaction 同时要求 Project Lock 与 Account Lock；同一 Account 或同一 Project 均不能并行。
- GUI 关闭不应结束 Daemon、`tmux` 或受管 Codex 任务；`muxlane` CLI 设计为独立诊断与 Recovery 入口。
- GUI/CLI 到 Daemon 使用版本化本地 JSON-RPC；Windows Host 只执行固定 `wsl.exe --exec /usr/bin/env muxlane|muxlaned` 入口；Terminal 使用独立、有界、typed 的 stdio data plane 和 tmux Control Mode。
- Phase 7 只提供非秘密模板/预设/输入历史和严格受 Project root 限制的只读文件导航；Asset/CodeMirror/文件写入已由 ADR-0013 延期。

Supported Target 是 Windows 10/11、WSL2、默认 Ubuntu WSL 发行版和 Windows x64。Windows MSVC/Tauri production build 与 Windows→WSL GUI/Terminal/托盘链路已验收；安装包、签名、自动更新和正式发布仍属于 Phase 8。

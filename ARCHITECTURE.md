# Muxlane 架构入口

Muxlane 已完成阶段 1 设计冻结、阶段 2/3 POC，以及 Phase 4/5 正式 WSL Runtime Control Plane 的本地实现与验收。Phase 6 产品 GUI 尚未开始；阶段关闭仍以 PR/CI 和 main 合并事实为准。

正式总体架构见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，需求范围见 [docs/PRD.md](docs/PRD.md)，运行安全与恢复冻结见 [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)、[docs/RUNTIME_LIFECYCLE.md](docs/RUNTIME_LIFECYCLE.md) 和 [docs/RECOVERY_STATE_MACHINE.md](docs/RECOVERY_STATE_MACHINE.md)，逻辑协议/数据模型/兼容策略见 [docs/PROTOCOL.md](docs/PROTOCOL.md)、[docs/DATA_MODEL.md](docs/DATA_MODEL.md) 和 [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md)，长期决策见 [docs/adr/README.md](docs/adr/README.md)。

## 架构摘要

- `Muxlane.exe` 是 Windows GUI；WSL 默认发行版内的单一 `muxlaned` 是 Runtime Control Plane。
- 每个 Project 设计为拥有永久的 Project Runtime 和 Project-scoped `CODEX_HOME`；Account 设计为独立 Account Vault。
- Launch Transaction 同时要求 Project Lock 与 Account Lock；同一 Account 或同一 Project 均不能并行。
- GUI 关闭不应结束 Daemon、`tmux` 或受管 Codex 任务；`muxlane` CLI 设计为独立诊断与 Recovery 入口。
- GUI/CLI 到 Daemon 使用版本化本地 JSON-RPC；Terminal 使用独立、有界、typed 的 stdio data plane 和 tmux Control Mode。

Supported Target 是 Windows 10/11、WSL2、默认 Ubuntu WSL 发行版和 Windows x64。Phase 4/5 已在 Windows MSVC/Tauri 与隔离 WSL 环境验收；安装包、签名、正式 UI 和发布仍不在当前交付范围。

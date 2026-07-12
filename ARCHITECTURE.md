# Muxlane 架构入口

Muxlane 当前处于阶段 1 的设计冻结工作。阶段 0 的 monorepo、质量工具链和最小桌面外壳已经存在；Account、Project Runtime、Daemon、终端、凭证和协议等业务能力尚未实现。

正式总体架构见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，需求范围见 [docs/PRD.md](docs/PRD.md)，运行安全与恢复冻结见 [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md)、[docs/RUNTIME_LIFECYCLE.md](docs/RUNTIME_LIFECYCLE.md) 和 [docs/RECOVERY_STATE_MACHINE.md](docs/RECOVERY_STATE_MACHINE.md)，逻辑协议/数据模型/兼容策略见 [docs/PROTOCOL.md](docs/PROTOCOL.md)、[docs/DATA_MODEL.md](docs/DATA_MODEL.md) 和 [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md)，长期决策见 [docs/adr/README.md](docs/adr/README.md)。

## 架构摘要

- `Muxlane.exe` 是 Windows GUI；WSL 默认发行版内的单一 `muxlaned` 是 Runtime Control Plane。
- 每个 Project 设计为拥有永久的 Project Runtime 和 Project-scoped `CODEX_HOME`；Account 设计为独立 Account Vault。
- Launch Transaction 同时要求 Project Lock 与 Account Lock；同一 Account 或同一 Project 均不能并行。
- GUI 关闭不应结束 Daemon、`tmux` 或受管 Codex 任务；`muxlane` CLI 设计为独立诊断与 Recovery 入口。
- GUI 到 Daemon 的控制面设计为版本化本地 JSON-RPC；Windows 到 WSL 的具体桥接仍待阶段 3 POC 验证。

设计目标不等于已交付功能。MVP 范围仅面向 Windows 10/11、WSL2、默认发行版和 Windows x64。

# Muxlane

Muxlane 是一个面向 Windows 与 WSL 的轻量 Codex Runtime 工作台，目标是为项目级运行时隔离、持久终端和配置资产治理提供清晰的本地控制面。

> **Pre-alpha：阶段 1 已冻结，阶段 2/3 POC 已验证；Phase 4/5 核心后台正在独立分支实现，但真实 WSL terminate、正式 Terminal 数据面、Windows 集成与真实账号 Usage 门禁尚未关闭，不能作为正式工具使用。**

Muxlane 由 [Mingll22](https://github.com/Mingll22) 独立开发，是非官方开源项目；与 OpenAI 无隶属关系，也不暗示 OpenAI 的合作、赞助或认证。

## 目标平台

- Windows 10/11
- WSL2
- 首发目标为 Windows x64

## 核心方向

- Windows GUI：`Muxlane.exe`
- WSL Runtime Control Plane：`muxlaned`
- WSL CLI：`muxlane`
- 项目级 `CODEX_HOME` 与本地配置资产治理
- 持久终端工作区和受控的账号凭证切换

这些方向目前仍是长期设计目标，不代表正式交付。当前仓库包含阶段 0 工程骨架、阶段 1 冻结设计，以及明确标记为非生产的阶段 2/3 POC。

## 不是什么

- 不是团队账号池。
- 不是自动轮换账号工具。
- 不是云端凭证托管服务。
- 不是完整 IDE。
- 不是 OpenAI 官方项目。

## Monorepo 结构

```text
apps/desktop/              Tauri 2 + React 桌面应用外壳
crates/muxlane-core/       正式领域、SQLite、Vault、锁、Recovery 与 Terminal 后台
crates/muxlane-protocol/   Protocol 1.0 typed control boundary 与兼容 POC frame
crates/muxlaned/           WSL control-plane daemon、Runner 与 Terminal compatibility gateway
crates/muxlane-cli/        JSON CLI、诊断与恢复入口
docs/                      架构、ADR 与研究文档入口
.github/                  CI、Issue/PR 模板和 Dependabot
```

## 设计文档

- [产品需求文档](docs/PRD.md)：阶段 1A 的范围、需求编号、验收原则与风险。
- [总体架构](docs/ARCHITECTURE.md)：系统边界、运行模型、数据布局与 POC 风险。
- [威胁模型](docs/THREAT_MODEL.md)：阶段 1B 的资产、边界、攻击路径与安全测试映射。
- [Runtime 生命周期](docs/RUNTIME_LIFECYCLE.md)：阶段 1B 的受管 Launch、停止、GUI 与 Daemon 生命周期。
- [持久恢复状态机](docs/RECOVERY_STATE_MACHINE.md)：阶段 1B 的事务状态、Hash 冲突和幂等 Recovery。
- [逻辑控制协议](docs/PROTOCOL.md)：阶段 1C 的 Control Plane、Terminal Data Plane 和能力协商候选。
- [逻辑数据模型](docs/DATA_MODEL.md)：阶段 1C 的实体、事实来源、SQLite 边界和迁移原则。
- [兼容策略](docs/COMPATIBILITY.md)：阶段 1C 的支持范围、能力探测和验证矩阵。
- [架构决策记录](docs/adr/README.md)：阶段 1 已接受的长期设计决策。
- [阶段 3 Terminal POC](docs/poc/phase-3-terminal/README.md)：Windows Tauri、WSL Gateway、tmux Control Mode 与生命周期验证结果。
- [架构摘要](ARCHITECTURE.md)：上述文档的根目录入口。

设计文档中的“设计为”“计划”或“要求”均不是已实现功能。

## 开发环境

- Node.js `22.22.2`，由 [.node-version](.node-version) 固定
- pnpm `10.16.1`
- Rust `1.97.0`，由 [rust-toolchain.toml](rust-toolchain.toml) 固定
- Windows 桌面开发还需要 Tauri 2 的平台依赖；阶段 0 不生成安装包

## 安装与验证

```bash
pnpm install --frozen-lockfile
pnpm verify
```

也可以分别执行：

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

`pnpm verify` 会在缺少 Linux Tauri 系统依赖的 WSL 环境中验证所有可移植 crate；原生桌面 Rust 检查由 Windows CI 执行。具备本机 Tauri 依赖时可额外运行 `pnpm verify:desktop`。

在隔离数据根中运行 daemon 与 CLI：

```bash
export MUXLANE_DATA_DIR=/absolute/wsl/path/to/muxlane-data
cargo run -p muxlane-cli -- daemon start
cargo run -p muxlane-cli -- doctor
cargo run -p muxlane-cli -- status
cargo run -p muxlane-cli -- daemon stop
```

CLI 还提供 `account list/import`、`project list/register`、`launch list/start`、`terminal list/create/history`、`usage probe/read/refresh`、`recover` 与 `diagnostics export`。Account import 只应指向用户明确选择的凭证文件；测试使用仓库中的不可认证 fixture。

## 开发路线

| 阶段 | 范围                                           |
| ---- | ---------------------------------------------- |
| 0    | 仓库奠基与质量基础设施                         |
| 1    | 需求与架构设计冻结                             |
| 2    | Project Runtime、凭证刷新与 Account 接管 POC   |
| 3    | Terminal、Windows—WSL Bridge、重连与背压 POC   |
| 4    | 锁、Launch Transaction、故障注入与冲突恢复 POC |
| 5    | 正式后台、SQLite、控制协议与 CLI               |
| 6    | GUI 与 Usage                                   |
| 7    | 工作台与 Asset                                 |
| 8    | 发布、安全、性能与运维                         |

阶段 1 已冻结需求、总体架构、威胁模型、Runtime 生命周期、恢复状态机、逻辑协议、数据模型、兼容策略和 ADR-0001～0012。阶段 2/3 POC 已验证有限 Runtime 与持久终端假设。Phase 4/5 分支已实现 SQLite、双锁、Launch/Credential/Recovery、daemon、CLI 和 Usage probe 的主体，但阶段仍因真实 WSL terminate、正式 Terminal 数据面、Windows 集成与真实账号 smoke 未完成而保持 `BLOCKED`；不得进入 Phase 6。

## 贡献

欢迎通过 [CONTRIBUTING.md](CONTRIBUTING.md) 了解环境、验证、提交和 PR 要求。架构决策应记录在 [docs/adr](docs/adr/README.md)。

## 安全

请阅读 [SECURITY.md](SECURITY.md)。不要在公开 Issue 中提交 Token、`auth.json`、Cookie、私钥或敏感日志。

## License

本项目采用 [Apache-2.0](LICENSE) 许可证。

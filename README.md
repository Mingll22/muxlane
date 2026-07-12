# Muxlane

Muxlane 是一个面向 Windows 与 WSL 的轻量 Codex Runtime 工作台，目标是为项目级运行时隔离、持久终端和配置资产治理提供清晰的本地控制面。

> **Pre-alpha：阶段 0 的工程基础已完成；当前处于阶段 1 的需求与架构冻结。正式业务能力尚未实现，不能作为正式工具使用。**

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

这些方向目前是设计目标，不代表已经交付。当前仓库包含阶段 0 工程骨架和阶段 1 设计文档。

## 不是什么

- 不是团队账号池。
- 不是自动轮换账号工具。
- 不是云端凭证托管服务。
- 不是完整 IDE。
- 不是 OpenAI 官方项目。

## Monorepo 结构

```text
apps/desktop/              Tauri 2 + React 桌面应用外壳
crates/muxlane-core/       后续共享核心边界（阶段 0 无领域模型）
crates/muxlane-protocol/   后续协议边界（阶段 0 无 RPC 契约）
crates/muxlaned/           WSL daemon 的元数据 CLI 骨架
crates/muxlane-cli/        WSL CLI 的元数据 CLI 骨架
docs/                      架构、ADR 与研究文档入口
.github/                  CI、Issue/PR 模板和 Dependabot
```

## 设计文档

- [产品需求文档](docs/PRD.md)：阶段 1A 的范围、需求编号、验收原则与风险。
- [总体架构](docs/ARCHITECTURE.md)：系统边界、运行模型、数据布局与 POC 风险。
- [架构决策记录](docs/adr/README.md)：当前冻结候选的长期决策。
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

CLI 骨架仅支持无副作用的元数据输出：

```bash
cargo run -p muxlane-cli -- --help
cargo run -p muxlane-cli -- --version
cargo run -p muxlaned -- --help
cargo run -p muxlaned -- --version
```

## 开发路线

| 阶段 | 范围                   |
| ---- | ---------------------- |
| 0    | 仓库奠基与质量基础设施 |
| 1    | 领域和持久化边界设计   |
| 2    | 项目级运行时隔离       |
| 3    | 凭证治理与锁定策略     |
| 4    | 启动事务与恢复机制     |
| 5    | 持久终端工作区         |
| 6    | 账号状态与额度查询     |
| 7    | 配置资产治理           |
| 8    | 发布、升级与运维完善   |

阶段 1A 冻结需求、总体架构和 ADR 候选；具体协议、状态机和持久化实现仍须按后续阶段验证。

## 贡献

欢迎通过 [CONTRIBUTING.md](CONTRIBUTING.md) 了解环境、验证、提交和 PR 要求。架构决策应记录在 [docs/adr](docs/adr/README.md)。

## 安全

请阅读 [SECURITY.md](SECURITY.md)。不要在公开 Issue 中提交 Token、`auth.json`、Cookie、私钥或敏感日志。

## License

本项目采用 [Apache-2.0](LICENSE) 许可证。

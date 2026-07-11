# Muxlane

Muxlane 是一个面向 Windows 与 WSL 的轻量 Codex Runtime 工作台，目标是为项目级运行时隔离、持久终端和配置资产治理提供清晰的本地控制面。

> **Pre-alpha：当前仅完成阶段 0 的仓库奠基，尚不可作为正式工具使用。**

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

这些方向尚未在阶段 0 实现。当前仓库只包含可验证的工程骨架。

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

更多当前边界见 [ARCHITECTURE.md](ARCHITECTURE.md)。

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

阶段 1 以前不会冻结正式协议或数据模型。

## 贡献

欢迎通过 [CONTRIBUTING.md](CONTRIBUTING.md) 了解环境、验证、提交和 PR 要求。架构决策应记录在 [docs/adr](docs/adr/README.md)。

## 安全

请阅读 [SECURITY.md](SECURITY.md)。不要在公开 Issue 中提交 Token、`auth.json`、Cookie、私钥或敏感日志。

## License

本项目采用 [Apache-2.0](LICENSE) 许可证。

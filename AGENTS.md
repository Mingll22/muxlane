# Muxlane Agent Guide

## 定位与阶段

Muxlane 是面向 Windows 与 WSL 的轻量 Codex Runtime 工作台。当前处于阶段 1：需求与架构设计冻结；阶段 0 的工程基础已完成。阶段 1 的设计文档不是业务实现：不要提前实现账号、凭证、项目注册、`CODEX_HOME`、协议、daemon 行为、终端、tmux、额度、配置资产治理、更新或发布能力。后续 POC 若推翻冻结假设，必须通过新的 ADR 修订，不能静默改写已接受决策。

## 目录职责

- `apps/desktop`：Tauri 2 + React 桌面外壳；首版 UI 使用简体中文。
- `apps/desktop/src-tauri`：最小原生入口和最小权限配置。
- `crates/muxlane-core`：共享核心边界；阶段 0 没有正式领域模型。
- `crates/muxlane-protocol`：未来协议边界；阶段 0 没有 RPC 契约。
- `crates/muxlaned`：WSL daemon 二进制边界。
- `crates/muxlane-cli`：WSL CLI 二进制边界。
- `docs/adr`：长期架构决策记录。

## Git 与验证

开始前必须执行：

```bash
git status --short && git branch --show-current && git log --oneline -5
```

不要覆盖、删除或混入用户已有改动。代码和提交信息使用英文，README/UI 首版使用简体中文。完成前运行匹配范围的真实验证；公共配置变更应运行：

```bash
pnpm install --frozen-lockfile
pnpm verify
```

`pnpm verify` 是 WSL/本地基础验证：它覆盖前端和非 Desktop Rust crates，不等于完整 Cargo Workspace 验证或 Tauri 原生构建验证。Desktop crate 由 Windows CI 或安装了完整 Tauri 系统依赖的环境验证；完成验收仍以匹配范围的完整 CI 结果为准。

不要声称运行了未实际运行的测试，也不要用跳过、吞错或空脚本伪造通过。

## 依赖治理

- 新增或升级 JavaScript、Rust、工具链与 GitHub Actions 依赖时，先查询官方 Registry、仓库或 Release，默认选择当时最新稳定版本；预发布版本仅在明确批准时使用。
- Manifest 保持项目的精确版本策略并提交对应锁文件；GitHub Actions 必须来自官方来源，以完整不可变 commit SHA 固定，并在行尾标注已验证的 Release tag。
- 不为追新批量刷新无关依赖。变更必须通过匹配范围的格式、静态检查、测试、构建、安全审计和 CI。
- 最新稳定版本存在兼容性问题时，保留当前固定版本，记录真实阻塞日志，并创建可追踪的 Issue 或文档记录后再暂缓升级。

## 安全与来源

- 禁止提交 `auth.json`、Token、Cookie、私钥、证书、真实用户路径、敏感日志或诊断包。
- 不在 UI、源代码或构建时环境变量中放置秘密。
- 不复制 Lampese/codex-switcher、CC Switch 或其他竞品的代码、UI、素材、文案或数据模型。
- Tauri 权限遵循最小化原则；不得在没有真实需求时增加 shell、文件系统、网络、进程或通用命令执行能力。

## 设计约束

- 不让 Demo 级假实现进入主分支。
- 新依赖必须有当前调用方和明确用途。
- 影响长期边界的架构决策需要 ADR。
- 阶段 1 前不得冻结未经讨论的正式协议和数据模型。

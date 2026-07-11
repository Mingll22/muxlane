# 贡献指南

## 环境准备

使用 [README.md](README.md) 所列 Node、pnpm 和 Rust 固定版本。安装依赖后运行本地基础验证：

```bash
pnpm install --frozen-lockfile
pnpm verify
```

`pnpm verify` 覆盖前端检查和非 Desktop Rust crates 的检查、Clippy 与测试；它不等于完整 Cargo Workspace 验证，也不等于 Tauri 原生构建验证。Desktop crate 由 Windows CI 或安装了完整 Tauri 系统依赖的环境验证。

提交前至少运行与改动范围匹配的检查；修改 Rust 或前端公共配置时应运行 `pnpm verify`，并以完整 CI 结果完成验收。

## 依赖治理

- 新增或升级 JavaScript、Rust、工具链和 GitHub Actions 依赖前，查询官方 Registry、仓库或 Release；默认采用当时最新稳定版本，不使用预发布版本，除非任务明确要求。
- 保持 Manifest 的精确版本策略并提交锁文件。GitHub Actions 仅使用官方来源，以完整不可变 commit SHA 固定，并在行尾注明对应 Release tag。
- 不批量升级无关依赖；变更必须通过格式、静态检查、测试、构建、安全审计和 CI。若最新稳定版本不兼容，记录阻塞日志、当前固定版本与可追踪的 Issue 或文档记录。

## 分支与提交

- 从已确认基线创建短生命周期分支，不要覆盖其他贡献者的未提交改动。
- 提交信息使用英文并遵循 Conventional Commits，例如 `feat: add project registry boundary`。
- 一个提交应保持单一、可验证的目的。
- 不使用 `git add .`；只暂存本次相关文件。

## Pull Request 最小要求

- 说明所属开发阶段和变更摘要。
- 列出实际运行的测试命令及结果。
- 说明安全影响、新增依赖和文档变更。
- UI 变化提供截图；非 UI 变化不需要截图。
- 确认没有跨阶段实现未批准能力。

## 安全与来源

- 不提交 `auth.json`、Token、Cookie、私钥、证书、真实日志或真实用户路径。
- 不复制 Lampese/codex-switcher、CC Switch 或其他竞品的代码、素材、文案、布局、图标、数据库结构或实现。
- 新依赖必须有当前阶段的真实用途，并在 PR 中说明。

本仓库当前未采用 DCO 或 CLA；除非仓库明确引入，不要自行添加。

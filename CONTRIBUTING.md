# 贡献指南

## 环境准备

使用 [README.md](README.md) 所列 Node、pnpm 和 Rust 固定版本。安装依赖后运行完整验证：

```bash
pnpm install --frozen-lockfile
pnpm verify
```

提交前至少运行与改动范围匹配的检查；修改 Rust 或前端公共配置时应运行完整 `pnpm verify`。

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

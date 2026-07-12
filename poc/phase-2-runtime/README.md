# 阶段 2A Runtime POC Harness

这是非生产化的阶段 2A Harness。它只创建空目录、检查路径和权限、输出文件元数据与 Hash，并在受控临时目录中运行 Codex CLI 的 `--version` / `--help` 探测；若当前 CLI 明确暴露 Schema 导出入口，则只在 disposable 目录生成并立即删除 Schema，保留元数据证据。

它不创建、复制、移动、读取或修改真实 `auth.json`，也没有 Account Vault、Credential Checkout / Commit、锁、事务、Recovery、tmux、Daemon、RPC 或 GUI 实现。

完整操作说明和安全边界见 [文档入口](../../../docs/poc/phase-2-runtime/README.md)。

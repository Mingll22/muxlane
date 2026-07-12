# 阶段 2 Runtime POC Harness

这是非生产化的阶段 2 Harness。Shell 工具创建空目录、检查路径/权限并执行无副作用 CLI 探测；`credential_harness.py` 使用 Python 3 标准库实现用户批准 source 到 POC Vault 的导入、Credential Checkout/Commit、Hash 冲突保留和不含值的 JSON 结构比较。

阶段 2A Shell 工具仍不接触真实凭证。Credential Harness 只处理导入后的 POC Vault 副本，不移动或修改用户 source；它没有正式 Account Repository、锁、durable transaction、Recovery、tmux、Daemon、RPC 或 GUI 实现。

完整操作说明和安全边界见 [文档入口](../../../docs/poc/phase-2-runtime/README.md)。

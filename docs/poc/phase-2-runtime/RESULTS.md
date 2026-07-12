# 阶段 2 Runtime POC 脱敏结果

> 状态：本地 POC 为 **PASS WITH LIMITATION**。本文记录本地结果；阶段正式关闭由对应 PR 审查、全部必要 CI、合并结果和合并后 main CI 独立确认。本文不包含真实路径、凭证 Hash、身份字段、Session/Thread ID、Prompt 或原始 evidence。

## 环境与能力

| 项目                     | 结果                                 |
| ------------------------ | ------------------------------------ |
| Codex CLI                | `0.144.1`                            |
| 创建入口                 | `codex exec --json`，持久 Session    |
| 恢复入口                 | `codex exec resume`，显式 Session ID |
| 模型 / Reasoning         | `gpt-5.4-mini` / `low`               |
| Sandbox                  | `read-only`                          |
| Project Runtime 文件系统 | WSL Linux `ext4`                     |
| syscall 级全局 Home 访问 | `NOT VERIFIED`（`strace` 不可用）    |

三个候选 OAuth 凭证经本地不可逆身份材料比较确认代表三个不同账号。当前开发 Session 使用 API-key-only 认证上下文，无法与 OAuth 账号指纹作同类型比较；该差异不影响两个不同 OAuth 账号之间的 2C 验证。

最低成本能力矩阵：

| 代号      | 认证/真实 Turn                 | 5 小时与周窗口                                | 选择     |
| --------- | ------------------------------ | --------------------------------------------- | -------- |
| Account A | 有效 Turn PASS                 | 精确窗口与 reset `NOT VERIFIED`；请求可用     | 2B、2C   |
| Account B | 有效 Turn PASS                 | 精确窗口与 reset `NOT VERIFIED`；请求可用     | 2C       |
| 未选候选  | 首次最小请求因额度失败；未重试 | 当前请求窗口不可用；精确 reset `NOT VERIFIED` | 不再测试 |

## Credential 与 Runtime

| 验证项                         | 结果                                                                  |
| ------------------------------ | --------------------------------------------------------------------- |
| 用户批准 source 导入 POC Vault | PASS；source 未修改                                                   |
| POC Vault / Runtime 根目录     | PASS，`0700`                                                          |
| Vault / Runtime `auth.json`    | PASS，普通文件、当前用户、`0600`、非 symlink                          |
| 原子 Checkout                  | PASS；同目录临时文件、文件/目录 `fsync`、原子 rename                  |
| 原子 Commit                    | PASS；POC Vault 备份、Hash 守卫、签回验证与 Runtime 清理              |
| Hash conflict                  | PASS；Vault 不覆盖、Runtime/backup 保留、结果为 `credential_conflict` |
| Project-scoped `CODEX_HOME`    | PASS；真实 Session 与内部状态写入对应 Project Runtime                 |
| Runtime 活动凭证清理           | PASS；所有正常与受控失败测试结束后均无活动 `auth.json`                |

## Session Continuity 与隔离

| 场景                                        | 结果                                              |
| ------------------------------------------- | ------------------------------------------------- |
| Account A 创建 Project A Session            | PASS；有效 synthetic Turn                         |
| Account A 恢复同一 Session                  | PASS；同一 Session 完成新 Turn                    |
| Account B 接管同一 Project A Runtime        | PASS；恢复 Account A 的同一 Session 并完成新 Turn |
| A → B → A                                   | PASS                                              |
| A → B 重复接管                              | PASS                                              |
| Project B 独立 Session                      | PASS                                              |
| Project A/B Session/marker 双向隔离         | PASS                                              |
| Project A/B synthetic history/config marker | PASS；不同 inode，修改不串用                      |
| Project A/B SQLite/internal state           | PASS；两边均存在且 inode 集合不交叉               |
| cross-runtime symlink / hard link           | PASS；Project A/B 均未观察到                      |

## Credential Mutation

结论：**NOT OBSERVED**。

所有真实有效 Turn、同 Session 恢复、跨账号接管和重复测试前后，整体 Hash 与结构级比较均未观察到 Credential 相关字段变化。因此结论只能是：

> No credential file mutation was observed during this run.

这不表示 Token Refresh 失败，也不证明未来运行不会自然刷新。synthetic fixture 只验证原子 Commit 机制，没有冒充真实 Refresh。

## 重复与失败场景

- Account A 的 Checkout/Commit 已重复超过两次；A/B 接管重复通过。
- 已验证 Runtime 已有凭证、Vault 缺失/错误 mode/owner、Vault/Runtime symlink、父路径 symlink、相似路径前缀、临时文件创建/写入失败、Runtime/Vault 目录错误 mode、重复 Checkout/Commit 和错误输出脱敏。
- Vault 在 Checkout 后变化时，Commit 返回 `credential_conflict`，不覆盖 Vault，并保留 Runtime 和 `0600` backup。
- Codex 因额度非零退出时，Runtime auth 保留到显式 Commit；没有自动覆盖或丢失。
- 不存在的 Session 恢复失败时，原 Session 文件摘要不变，Runtime auth 保留到显式 Commit。
- Project Isolation 文件系统检查重复通过。一次本地审计曾错误把 disposable capability-probe Runtime 的内部链接纳入 A/B 范围；原始 FAIL evidence 被保留，修正范围后的 A/B 复核 PASS。

## 结论与边界

本地证据支持阶段 2 的 Project-scoped `CODEX_HOME`、Credential Checkout/Commit、Account A/B Session Continuity 与 Project Runtime Isolation 假设，没有发现需要修订阶段 1 ADR 的反例。由于本次未自然观察到 Credential Mutation，结论是 **PASS WITH LIMITATION**。

该结论不覆盖阶段 3 的 Terminal/Bridge，也不覆盖阶段 4 的双锁、durable transaction、Daemon/Runner kill、WSL 重启与自动 Recovery；不应把本 POC Harness 包装为阶段 5 正式 Runtime Manager。

## 本地质量门禁

| 命令 / 检查                                    | 状态                                                           |
| ---------------------------------------------- | -------------------------------------------------------------- |
| Shell `bash -n`                                | PASS                                                           |
| `python3 -m compileall -q poc/phase-2-runtime` | PASS                                                           |
| Credential Harness unittest                    | PASS，29 tests                                                 |
| 阶段 2A Harness self-test                      | PASS                                                           |
| ShellCheck                                     | `NOT RUN`，当前环境未安装                                      |
| `pnpm install --frozen-lockfile`               | PASS                                                           |
| `pnpm verify`                                  | PASS；保留既有 Vite chunk warning                              |
| `pnpm audit --prod --audit-level=high`         | PASS，无已知 vulnerability                                     |
| `cargo audit`                                  | PASS（exit 0）；17 个仓库已知 allowed warning                  |
| `pnpm verify:desktop`                          | `BLOCKED`（exit 101）；WSL 缺少 `pkg-config`/GTK/GLib 系统依赖 |
| 自定义敏感信息与 Unicode 控制字符扫描          | PASS                                                           |
| Gitleaks                                       | `NOT RUN`，当前环境未安装                                      |

本轮没有新增第三方依赖。Desktop Rust 的完成证据仍依赖 Windows CI；PR 与 main CI 状态不冻结在本地 POC 文档中，由阶段关闭记录单独确认。

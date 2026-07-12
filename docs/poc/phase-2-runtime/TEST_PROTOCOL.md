# 阶段 2 Runtime POC 测试协议

> 此协议定义 POC 的可审计步骤。阶段 2B～2D 已按本协议执行；公开结果见 [RESULTS.md](RESULTS.md)。测试数据只使用 `Account A`、`Account B`、`Project A`、`Project B` 等代号；日志和报告使用 `<POC_ROOT>`、`<CODEX_HOME>` 等占位符。

## 通用安全门与证据格式

每次 2B/2C/2D 测试开始前必须确认：

1. POC Root 位于 WSL Linux 原生文件系统，非仓库、非源码树、非 `/mnt/c`、非 `/mnt/d`、非同步目录。
2. Vault、Project Runtime 和 evidence 目录均为 `0700`；受控的普通 `auth.json` 文件为 `0600`；所有目标均拒绝符号链接。
3. 真实凭证只在用户明确批准的受控副本中使用；不得移动 Vault 原件，不得把内容写入日志、SQLite、诊断包或 Git。
4. 每一步先记录意图和前置 Hash/元数据；退出码非零、身份不明、权限异常或 Hash 冲突一律停止并归类为 `FAIL` 或 `BLOCKED`，不得自动覆盖。

每条证据记录必须包含：测试 ID、时间、Codex CLI 版本、Project ID、Account 测试代号、`CODEX_HOME` 占位路径、命令、退出码、文件元数据、SHA-256、预期、实际、`PASS` / `FAIL` / `BLOCKED` / `NOT VERIFIED` 与脱敏说明。不得记录完整 `auth.json`、Token、Cookie、Authorization Header、真实邮箱、真实 Prompt、真实 Codex 会话内容或用户本地隐私路径。

## 2B：单 Account A 受控测试协议

| 步骤  | 操作                                                                                          | 证据与判定                                                                                |
| ----- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| 2B-01 | 初始化新的受控 POC Root，记录 Vault 与 Project A Runtime 的目录模式。                         | `0700`、常规目录、无符号链接，否则 `FAIL`。                                               |
| 2B-02 | 在用户已批准且不提交的 Account A Vault 副本上记录 `auth.json` 元数据与 SHA-256。              | 不打印内容；Vault 原件不移动。                                                            |
| 2B-03 | 以同目录、原子复制方案把凭证复制到 Project A Runtime。                                        | 此步骤是后续 POC，不由 2A Harness 实现；记录前后 Hash、临时文件/rename/fsync 证据。       |
| 2B-04 | 显式设定 `CODEX_HOME=<Project A Runtime>`，启动受控 Codex。                                   | 确认进程实际使用独立 Runtime；不能只凭命令行猜测。                                        |
| 2B-05 | 创建可识别、无隐私内容的测试会话。                                                            | 只记录测试 ID、退出码和安全的会话摘要。                                                   |
| 2B-06 | 正常退出并记录 Runtime `auth.json` 的前后 SHA-256、mtime、模式和 owner。                      | Hash 改变只表示凭证文件变化；Hash 未变只表示本次未观察到变化。                            |
| 2B-07 | 在 Account A 未变化、Runtime 合格及 Codex 已确认退出时，执行原子签回并清理 Runtime 活动凭证。 | 记录 Hash、目录持久化和清理；失败不标记完成。                                             |
| 2B-08 | 判断是否观察到 refresh。                                                                      | 只有安全的结构级比较且不泄露字段时才能称为“疑似 refresh”；无法触发时标记 `NOT VERIFIED`。 |

## 2C：Account A/B 对 Project A 的顺序接管协议

1. Account A 在 Project A Runtime 创建无隐私测试会话，记录 A 的 Vault/Runtime Hash 和安全 Session 标识。
2. Account A 正常退出，完成安全签回与 Runtime 活动凭证清理。
3. 以同一 Project A Runtime 为目标，将 Account B 的受控凭证副本签出；Project Runtime 的 sessions、history、config 和内部状态不得重建或删除。
4. 记录 B 签出前后的 Runtime Hash、A/B Vault Hash、文件模式与所有权。
5. Account B 尝试恢复 A 创建的同一项目会话；如实记录成功、失败或要求重新认证，绝不伪造“跨账号接管成功”。
6. Account B 正常退出并完成安全签回。验证 A/B Vault 均未串号，Runtime Hash 全程可审计。
7. 如任一 Vault 在非预期时点变化，或 A/B 同时出现两个合理的新版本，停止并保留证据，按 `credential_conflict` 原则人工处理。

## Project Runtime 隔离协议

| 测试 ID   | 操作                                                       | 预期与限制                                                             |
| --------- | ---------------------------------------------------------- | ---------------------------------------------------------------------- |
| 2C-ISO-01 | 为 Project A、Project B 分别设置不同的 `<CODEX_HOME>`。    | 路径、inode/元数据和文件系统边界可区分；不能共享目录。                 |
| 2C-ISO-02 | 在 A 创建无隐私会话/配置标记，在 B 读取自身 Runtime 状态。 | B 不读取 A 的 sessions、history、config、SQLite 或内部状态。           |
| 2C-ISO-03 | 反向执行 B 到 A 的检查。                                   | A 不读取 B 的状态。                                                    |
| 2C-ISO-04 | 记录两边 Hash、路径占位符、CLI 版本与退出码。              | 单次检查不能证明所有 Codex 内部文件格式；未可见项标记 `NOT VERIFIED`。 |

## Token Refresh 判定协议

1. 仅比较受控 Vault/Runtime 普通文件的 SHA-256、mtime、模式、owner 与安全文件类型。
2. Hash 或 mtime 改变时，结论是“观察到受控凭证文件变化”；只有不泄露内容的结构级安全比较支持时，才可进一步记录“疑似 Token refresh”。
3. Hash 未变化时，结论必须是“本次未观察到文件变化”，不能写成 refresh 已发生或未发生。
4. 无法触发刷新时记录 `NOT VERIFIED`；禁止手动编辑 `auth.json` 伪造 refresh，禁止把 synthetic fixture 的变化称为真实 refresh。
5. 任何 Vault/Runtime 双方变化都不得 last-write-wins。保留所有安全副本与 Hash 摘要，停止自动操作并进入人工冲突流程。

## 2D 前置的失败与路径测试

2D 才可在没有真实凭证的情况下扩展验证同目录 rename、文件/目录 `fsync`、权限变化、磁盘满和中断窗口。它仍不得实现正式 SQLite、锁、事务、Daemon 或 Recovery 服务。每个负向测试必须证明：无凭证内容泄露、不自动覆盖、失败退出码非零、证据已脱敏。

## 当前状态

- 真实 Credential Checkout/Commit、Account A/B 顺序接管、同 Session 有效 Turn、Project Runtime 隔离与适合阶段 2 的失败场景已经验证。
- Token/Credential Mutation 在本次真实运行中为 `NOT OBSERVED`，不是 Refresh 失败。
- syscall 级全局 Codex Home 访问为 `NOT VERIFIED`；当前环境没有 `strace`。
- Daemon/Runner 崩溃、WSL 重启、双锁、durable transaction 与 Recovery Manager 仍属于阶段 4，未由本协议声称通过。

# Muxlane 产品需求文档（PRD）

## 1. 文档状态

| 项目     | 内容                                                            |
| -------- | --------------------------------------------------------------- |
| 状态     | Draft / Frozen Candidate                                        |
| 对应阶段 | 阶段 1A：需求与总体架构冻结                                     |
| 维护者   | Mingll22 / Muxlane maintainers                                  |
| 最后更新 | 2026-07-12                                                      |
| 变更原则 | 冻结候选只可通过审查和 ADR 更新；未验证事实必须标为假设或 POC。 |

本文件定义产品范围和可验收需求；[总体架构](ARCHITECTURE.md) 定义系统边界，ADR 记录长期取舍。阶段 1 只冻结设计，不能据此宣称业务能力已实现。

## 2. 产品概述

Muxlane 是面向 Windows 与 WSL 的轻量 Codex Runtime 工作台：`Windows GUI + WSL Runtime Control Plane + Project-scoped CODEX_HOME + Persistent Terminal Workspace + Account and Configuration Governance`。它为 Codex CLI 个人开发者提供项目级运行时隔离、持久 Terminal、多 Account 凭证顺序切换、额度查询、Recovery 与 Asset 治理。

目标用户是 Windows 10/11、WSL2、同时维护多个本地项目的重度终端用户。它不是团队共享账号池、出租或出售账号工具、云端凭证托管、自动账号轮换/额度规避工具、完整 IDE、代码分析平台或 SaaS。

## 3. 问题陈述

直接使用 Codex CLI 时，多个项目容易混用 `CODEX_HOME`，导致 Session、配置和缓存缺乏项目隔离。多 Account 手工复制凭证会覆盖文件、触发 refresh 竞争并难以安全签回。GUI 关闭常与终端任务生命周期耦合，WSL 长任务、屏幕与输出也难以恢复。Skills、MCP、Plugins 与配置没有项目级治理，同时缺少统一的诊断和 Recovery 入口。

## 4. 目标与非目标

### 目标

- 每个已注册 Project 设计为拥有永久、隔离且位于 WSL Linux 文件系统的 Project Runtime。
- 一个运行中 Project 只能有一个受管 Codex 主实例；一个 Account 同时只能分配给一个运行中 Project。
- GUI 重连设计为可恢复 Project、Terminal、屏幕缓冲、历史输出和实时日志；CLI 不依赖 GUI。
- Account、Usage、Asset 与 Project 的操作设计为本地优先、可诊断且不保存 Token 到数据库。

### 非目标

不实现云端凭证托管、团队账号池、自动账号轮换、额度耗尽自动切号或规避限制；不做完整 IDE、LSP、Debugger、Git GUI 或 AI 自动补全。MVP 不支持多 WSL 发行版、macOS、原生 Linux GUI、Windows ARM、第一版完整自动更新或高级 Pane 编排。

## 5. 角色与核心场景

| 场景                          | 预期行为                                                                         |
| ----------------------------- | -------------------------------------------------------------------------------- |
| 单 Project、单 Account        | 取得双锁后启动隔离 Runtime，并在退出时完成 Credential Commit。                   |
| 同一 Project 顺序切换 Account | 先安全停止/提交前一 Account，再使用新 Account；Project Runtime 与 Session 保留。 |
| 多 Project 并行               | 仅在 Project 与 Account 均不同且各自双锁成功时并行。                             |
| GUI 关闭后任务继续            | GUI 可关闭、最小化到托盘或重启；Daemon 与 `tmux` 继续受管。                      |
| GUI 重启恢复 Terminal         | 重新连接并恢复 Project、Terminal、缓冲和实时日志。                               |
| Codex 退出                    | 识别退出，执行 Credential Commit、清理活动凭证并更新状态。                       |
| Windows/WSL 异常后 Recovery   | 依据事务、锁和进程身份执行幂等检查，不覆盖冲突凭证。                             |
| Usage 查看                    | 在受限查询环境按需查询并显示窗口及本地缓存时间。                                 |
| Project 归档                  | 保留注册、Runtime、Session、Terminal、日志和 Asset 配置，非立即物理删除。        |
| CLI 诊断                      | `doctor`、`status`、daemon 管理、列表、`recover`、诊断包导出均无需 GUI。         |

## 6. 功能需求

优先级：P0=MVP 必需，P1=v0.1.0，P2=后续。阶段是正式实现目标而非当前完成状态。

| 编号               | 描述与验收标准                                                                                                                                        | 优先级 / 阶段 | 安全或兼容约束                                                         | 依赖                           |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- | ---------------------------------------------------------------------- | ------------------------------ |
| FR-ACCOUNT-001     | 管理本人拥有的 Account 元数据和独立 Account Vault；验收：Vault 外不得出现 Token，Account 列表不泄露凭证。                                             | P0 / 3        | `accounts/<id>` 为 `0700`，`auth.json` 为 `0600`。                     | FR-CREDENTIAL-001              |
| FR-PROJECT-001     | 注册 Project 并永久映射 Project Runtime；验收：不同 Project 的 `CODEX_HOME`、Session 和缓存不混用。                                                   | P0 / 2        | Runtime 不得在源码、`/mnt/c`、`/mnt/d` 或同步目录。                    | FR-RUNTIME-001                 |
| FR-RUNTIME-001     | 为 Project 启动一套受管 Runtime；验收：同一 Project 的第二主实例被明确拒绝。                                                                          | P0 / 2、5     | 每项目最多一个 Codex 主实例。                                          | FR-LOCK-001                    |
| FR-CREDENTIAL-001  | 执行 Credential Checkout、Commit 与 Cleanup；验收：Vault 原件不移动，失败能留下可恢复事务和冲突副本。                                                 | P0 / 3、4     | 临时文件、fsync、同目录原子 rename、目录 fsync、Hash 和幂等 Recovery。 | FR-LOCK-001                    |
| FR-LOCK-001        | Launch Transaction 同时取得 Project Lock 和 Account Lock；验收：同 Account 并行和同 Project 多实例均被拒绝，不同 Project+不同 Account 可并行。        | P0 / 3        | Linux `flock` 为真相；禁止静默抢占、自动切号、心跳直接失效。           | FR-PROJECT-001, FR-ACCOUNT-001 |
| FR-RECOVERY-001    | Recovery 能检查中断事务、锁、进程身份和凭证 Hash；验收：重复执行不损坏状态或覆盖未知较新凭证。                                                        | P0 / 4        | PID 重用、WSL 重启和冲突保留必须处理。                                 | FR-CREDENTIAL-001              |
| FR-TERMINAL-001    | Project 拥有一个 `tmux` Session，MVP 可创建多个 Window；验收：GUI 关闭后任务继续，重连显示受限历史和实时输出。                                        | P0 / 5、6     | MVP 不支持 Pane；Terminal 输入经受控通道。                             | FR-RUNTIME-001                 |
| FR-USAGE-001       | 按需查询 Account Usage；验收：展示 `windowDurationMins` 语义的窗口、缓存时间和 Asia/Shanghai 时间，不假定未验证字段。                                 | P1 / 5、6     | 独立 Query Home、并发限制、短期缓存；禁止用作自动切号。                | FR-ACCOUNT-001                 |
| FR-ASSET-001       | 管理 Skill、MCP、Plugin、source、version、checksum、compatibility、enabled_projects、install_mode；验收：共享白名单与 Project 隔离可审计。            | P1 / 7        | 不引入未验证来源或越权安装。                                           | FR-PROJECT-001                 |
| FR-FILE-001        | 提供轻量文件树和 CodeMirror 6 查看/编辑；验收：Codex 主进程存在时内置编辑器只读。                                                                     | P1 / 7        | 受限于 Project 目录和 Tauri Capability。                               | FR-RUNTIME-001                 |
| FR-HISTORY-001     | 显示 Codex Session / Thread 与 Terminal 历史；验收：历史有上限且不上传 Prompt。                                                                       | P1 / 6、7     | 本地保存、脱敏日志、可归档。                                           | FR-TERMINAL-001                |
| FR-DIAGNOSTICS-001 | 提供 `muxlane doctor`、`status`、`daemon start/stop`、`project list`、`account list`、`recover`、`diagnostics export`；验收：无需 GUI，导出包先脱敏。 | P0 / 4、5     | 默认不上传；崩溃日志上传须主动授权。                                   | FR-RECOVERY-001                |

明确不做的需求包括自动轮换、额度规避、团队共享、云同步凭证和高级 Pane；不得以 P2 名义绕过非目标。

## 7. 非功能需求

| 编号            | 要求与可验证准则                                                                            | 阶段 |
| --------------- | ------------------------------------------------------------------------------------------- | ---- |
| NFR-SEC-001     | Token 不进入 SQLite、日志、遥测或诊断包；Vault/文件权限、原子凭证事务和双锁受测试。         | 3–5  |
| NFR-PRIVACY-001 | 默认无遥测；不上传 Prompt、Terminal 日志或文件路径；任何崩溃上传要求用户授权。              | 5–8  |
| NFR-REL-001     | 异常退出后 Recovery 幂等，事务可追溯；目标是凭证不串号、状态不静默丢失。                    | 4–5  |
| NFR-PERF-001    | GUI 常规启动和恢复须在阶段 POC 定义并量化目标；Daemon 单实例、App Server 按需且有资源上限。 | 3、6 |
| NFR-MAINT-001   | 协议、迁移、错误和日志具备版本策略；不为未使用能力引入依赖或复杂抽象。                      | 1–8  |
| NFR-TEST-001    | 锁、事务、Recovery、路径规范化和脱敏具备可重复的单元/集成/故障注入测试。                    | 2–8  |
| NFR-DIAG-001    | 结构化脱敏日志、健康状态、事务事件和进程身份可由 CLI 导出。                                 | 4–8  |
| NFR-COMPAT-001  | 仅宣称已测试的平台；兼容最近稳定 Codex CLI 的受验证能力，Schema 演进时降级或拒绝。          | 2–8  |
| NFR-BUILD-001   | 构建与依赖锁定可复现，质量门禁覆盖格式、静态检查、测试和适用平台构建。                      | 0–8  |

## 8. 并发、数据与凭证原则

Account 独占且 Project 独占；只有不同 Project 使用不同 Account 时可并行。锁冲突必须展示占用状态和安全处理建议，绝不静默抢占或自动改用其他 Account。心跳、GUI 内存和 SQLite 占用字段都不是锁失效的最终判断。

Project-scoped `CODEX_HOME` 位于 `~/.local/share/muxlane/projects/<project-id>/codex-home`。Account Vault 与 Runtime 活动 `auth.json` 分离：Checkout 原子复制，Codex 运行后原子 Commit，随后 Cleanup。Vault 不移动，数据库不保存 Token；Hash 不匹配时保留冲突而不覆盖。Project 删除只归档。

## 9. 平台与阶段映射

MVP 支持 Windows 10/11、WSL2、默认 WSL 发行版和 Windows x64；不宣称未测试平台。阶段 2 验证 Project Runtime，阶段 3 验证 Vault/锁/本地桥接，阶段 4 验证 Launch Transaction 与 Recovery，阶段 5 建立正式后台与 Terminal/CLI，阶段 6 建立 GUI/Usage，阶段 7 建立工作台和 Asset，阶段 8 做发布与运维。

## 10. 风险、假设与开放问题

**已冻结决策：** Project-scoped `CODEX_HOME`、独立 Account Vault、Project+Account 双锁、同 Account 不并行、单 Daemon、CLI Recovery、本地优先与无遥测默认值。

**当前假设：** 默认 WSL 发行版和当前 Linux 文件系统可满足权限与 `flock` 前提；当前安装 Codex CLI 可在隔离 `CODEX_HOME` 下工作。

**POC 风险：** Windows-WSL 本地桥接、`tmux` Control Mode 稳定性、`auth.json` refresh 行为、fsync 语义、WSL 重启、PID 重用、路径规范化、Codex CLI/App Server Schema 演进、Tauri Capability 与 Terminal 输入安全。

**开放问题：** 最终桥接传输、协议版本规则、数据库 Schema、Usage 字段与 App Server 用法，均留待阶段 1C/后续 POC，不能作为既定事实。Runtime 生命周期与恢复状态机见阶段 1B 文档，仍须通过阶段 2–4 POC 验证实现前提。

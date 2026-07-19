# Muxlane 项目真相

> 本文档是项目级长期上下文，不替代代码、Git 和运行验证。最近核验快照是时间点事实，每次任务仍须重验。

## 1. 项目定位

Muxlane 是面向 Windows 10/11 与默认 WSL2 发行版的本地 Codex Runtime 工作台。它以 Project-scoped `CODEX_HOME`、Account Vault、持久 tmux Terminal 和可恢复 Launch Transaction 为核心，不是团队账号池、自动轮换工具、云凭证服务或完整 IDE。

## 2. 核心使用场景

- 为本地 Project 注册稳定、隔离且位于 WSL 文件系统的 Runtime。
- 从用户明确选择的文件导入合成或本人持有的 Account 凭证副本，不继续引用源文件。
- 在 Account→Project 双 `flock` 下启动受管 Codex，退出后安全签回凭证。
- GUI/CLI 断开或 daemon 重启后，依据持久事务、进程身份和 Hash 重新分类。
- 通过 `muxlane` CLI 执行 health、status、注册、启动、Recovery、Usage 探测和诊断导出。

## 3. 当前范围与非目标

当前仓库包含阶段 0 工程基础、阶段 1 冻结设计、阶段 2/3 POC，以及 `feat/phase-4-5-core-runtime` 分支上的 Phase 4/5 未关闭实现。该分支已实现正式核心后台的大部分基础，但不代表 Phase 4/5 已通过全部硬门禁。

明确非目标仍包括 Phase 6 正式管理 UI/额度看板/托盘产品逻辑、Phase 7 Asset/CodeMirror/文件树工作台，以及 Phase 8 安装包、签名、更新和发布。

## 4. 当前架构

- `muxlaned`：当前 Linux 用户范围内的单实例 Unix Socket control plane；受管 Runner 在 tmux 内独立于 GUI/daemon 生命周期。
- `muxlane`：先握手再调用 typed JSON-RPC 的稳定 JSON CLI。
- `muxlane-core`：受控目录、SQLite migration、Account/Project/Launch/Terminal/Usage 模型、双锁、Credential transaction、Recovery、进程身份和诊断。
- `muxlane-protocol`：Protocol `1.0` 握手、capability negotiation、typed request/response/error；Phase 3 POC frame 暂时仍保留为兼容边界。
- SQLite 保存非秘密元数据、事务、索引和恢复审计；Filesystem 保存 Vault、Runtime、证据和日志；Linux `flock` 是活动互斥事实；tmux 是 Terminal 存活与历史载体。

## 5. 已实现能力

- SQLite schema v1→v2 事务迁移、约束、索引、`quick_check` 和新版本拒绝。
- `0700` 受控目录、`0600` DB/Socket/Vault/Runtime 文件、no-follow 检查与同目录原子写入。
- Account import、稳定 Project ID、Windows/WSL 路径映射和每 Project 独立 Runtime。
- Account→Project 双 `flock`、持久 Launch Transaction、Runner/Codex `boot_id + PID + start ticks + executable digest` 身份。
- Credential checkout/commit/cleanup、Vault/Runtime Hash 矩阵、冲突/损坏证据隔离和幂等 Recovery。
- 单实例 daemon、CLI 核心命令、协议握手/版本错误、operation ID 持久去重。
- 受管 tmux Session/Window、一次性有界 history bootstrap；Phase 3 POC 仍提供已验证的 Control Mode live stream。
- 控制消息在分配前执行 1 MiB 上限，并将同时连接的同用户客户端限制为 64。
- 当前 Codex 版本的 App Server schema probe、stdio-only Usage adapter、短期缓存模型和失败清理。
- 默认不含凭证、Prompt、Terminal 原文和源码路径的诊断导出。

## 6. 当前阶段与路线

### 当前阶段

Phase 4/5 实现与验证进行中，阶段结论为 `BLOCKED`，不得合并到 `main` 或进入 Phase 6。已通过的 Linux/WSL synthetic fault injection 不覆盖真实 `wsl --terminate`、Windows host lifecycle、正式 Terminal 数据面迁移和真实账号 Usage smoke。

### 后续粗粒度路线

1. 完成 Phase 4 剩余真实 WSL terminate / boot-change 故障注入并关闭恢复 POC。
2. 将 Phase 3 live Terminal compatibility frame 迁移为正式数据面，完成 Windows/WSL 集成、真实 Usage smoke、review 与 CI 后关闭 Phase 5。

## 7. 已知限制、风险与技术债

- 当前环境不能安全终止正在承载本会话的 WSL 发行版；`wsl --terminate` 真实门禁未运行。
- 正式 control plane 已实现 Terminal create/list/history，但 attach/switch/close、Session/Thread 索引和实时 Control Mode stream 尚未正式化；live stream 仍位于 Phase 3 POC compatibility surface。
- Usage 成功路径仅用 fixture 归一化测试；未读取全局 `~/.codex/auth.json`，真实账号 smoke 未运行。
- Usage 尚缺批量刷新与显式全局并发限制；Project archive 只有 schema 边界，尚无正式服务/协议命令。
- Recovery incident 当前可保留并阻止新 Launch，但尚无经过设计和验证的人工 resolve 工作流。
- Windows Desktop Rust、真实 Windows Tauri host、GitHub PR review/CI 和 post-merge main CI 尚未运行。
- `cargo audit` 无阻断漏洞，但仓库 allowlist 仍报告 17 条未维护/unsound 警告；包括 Tauri Linux GTK3 依赖的 `RUSTSEC-2024-0429`。
- 2026-07-19 一次失败的 shell 故障编排在默认 `~/.local/share/muxlane` 创建了空 DB/运行目录；确认 0 Account、0 Project、0 Launch，未擅自删除。

## 8. 关键工程约束

- 不把 Token、Cookie 或完整 `auth.json` 写入 SQLite、日志、RPC、诊断包或 Git。
- 锁顺序固定 Account→Project；锁文件、SQLite、heartbeat 和 GUI 状态不能替代真实 `flock`。
- Terminal transaction 终态不可改写；`credential_conflict` 必须保留证据并人工处理。
- 控制面仅 Unix Socket，不开放 LAN；WebView 不获得任意 Shell、文件或 WSL 权限。
- 不把 Phase 2/3 POC frame、Gateway 或 synthetic runner 描述为正式产品协议。

## 9. 下一步

在隔离 WSL 发行版完成真实 terminate/boot-change 注入，并将正式 Terminal data plane 接到受管 `muxlane-runtime` tmux Session；两项通过前不得创建合并 PR。

## 10. 最近核验快照

- 核验日期：2026-07-19
- 分支：`feat/phase-4-5-core-runtime`
- 起始 HEAD：`f0dfd1443ea87244ad2407c53998e90c87bea048`
- 工作区：阶段检查点提交完成；具体 HEAD 与清洁状态以当前 Git 为准
- 关键验证：39 个 core/protocol/daemon/CLI/Phase 3 compatibility 测试通过；5 组隔离 fault injection 通过，最近证据根 `/tmp/muxlane-phase45.tiw5Ao`；`pnpm verify` 已通过；Desktop Rust 在 WSL 因缺 `pkg-config`/GTK 系统库而 `BLOCKED`；Windows、PR/CI 未运行

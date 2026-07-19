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

当前仓库包含阶段 0 工程基础、阶段 1 冻结设计、阶段 2/3 POC，以及已关闭的 Phase 4/5 正式 WSL Runtime Control Plane。Phase 4/5 的本地 Linux、隔离 WSL、Windows MSVC/Tauri、跨边界验收、PR CI、squash merge 和合并后 `main` CI 均已完成。

明确非目标仍包括 Phase 6 正式管理 UI/额度看板/托盘产品逻辑、Phase 7 Asset/CodeMirror/文件树工作台，以及 Phase 8 安装包、签名、更新和发布。

## 4. 当前架构

- `muxlaned`：当前 Linux 用户范围内的单实例 Unix Socket control plane；受管 Runner 在 tmux 内独立于 GUI/daemon 生命周期。
- `muxlane`：先握手再调用 typed JSON-RPC 的稳定 JSON CLI。
- `muxlane-core`：受控目录、SQLite migration、Account/Project/Launch/Terminal/Thread/Usage 模型、双锁、Credential transaction、Incident resolution、Recovery、进程身份和诊断。
- `muxlane-protocol`：Protocol `1.0` control plane 和正式 Terminal data plane；Phase 3 POC frame 仅保留为隔离兼容测试边界。Windows host 使用 platform-neutral DTO，不链接 WSL-only core。
- SQLite 保存非秘密元数据、事务、索引和恢复审计；Filesystem 保存 Vault、Runtime、证据和日志；Linux `flock` 是活动互斥事实；tmux 是 Terminal 存活与历史载体。

## 5. 已实现能力

- SQLite schema v1→v4 事务迁移、约束、索引、`quick_check` 和未来版本拒绝；Terminal window identity 只在活动记录内唯一。
- `0700` 受控目录、`0600` DB/Socket/Vault/Runtime 文件、no-follow 检查与同目录原子写入。
- Account import、稳定 Project ID、Windows/WSL 路径映射和每 Project 独立 Runtime。
- Account→Project 双 `flock`、持久 Launch Transaction、Runner/Codex `boot_id + PID + start ticks + executable digest` 身份。
- Credential checkout/commit/cleanup、Vault/Runtime Hash 矩阵、冲突/损坏证据隔离和幂等 Recovery。
- 单实例 daemon、CLI 核心命令、协议握手/版本错误、operation ID 持久去重。
- 正式 Terminal create/list/attach/detach/switch/close、history/live、input/Ctrl+C/resize、背压、旧 stream 拒绝、跨 Project/Window 隔离和重连。
- Codex Session/Thread 元数据索引、Project 逻辑归档、显式且可审计的 Incident resolution；原始 Session 和终态 Transaction 不被改写。
- 控制消息在分配前执行 1 MiB 上限，并将同时连接的同用户客户端限制为 64。
- Codex App Server schema probe、stdio-only Usage adapter、每 Account Query Home、短期缓存、批量刷新和全局 4 并发限制；自动化使用 fake App Server。
- 默认不含凭证、Prompt、Terminal 原文和源码路径的诊断导出。

## 6. 当前阶段与路线

### 当前阶段

Phase 4/5 结论为 `PASS`，已通过 PR #12 squash merge 到 `main`，合并后 CI 全绿，阶段分支和隔离测试环境已清理。Phase 6 尚未开始。

### 后续粗粒度路线

1. 以独立任务启动 Phase 6 GUI 的账号、Project、Usage 和生命周期产品化。
2. 不提前进入 Skills/MCP/完整工作台或发布阶段。

## 7. 已知限制、风险与技术债

- WSL2 所有发行版共享 utility VM kernel；对专用发行版执行真实 `wsl --terminate` 不改变 kernel `boot_id`。因此 terminate 恢复与 boot identity change 分别验证：专用 WSL 执行真实 terminate，隔离 systemd-nspawn 启动提供两个真实且不同的 Linux boot identity。
- 真实账号 Usage success smoke 未获授权，状态为 `NOT RUN`；没有读取或复制全局 `~/.codex/auth.json`。fixture/fake App Server 的 schema、5h/周窗口、Reset Credit、Token、失败与并发路径已覆盖。
- Linux Desktop Rust 在当前 WSL 因缺 `pkg-config`/GTK 开发包仍为环境 `BLOCKED`；Windows MSVC Desktop check、Clippy、test、release build 和 native run 已通过，因此不构成 Windows 目标门禁失败。
- `cargo audit` 无阻断漏洞，但仓库 allowlist 仍报告 17 条未维护/unsound 警告；包括 Tauri Linux GTK3 依赖的 `RUSTSEC-2024-0429`。
- PR #12 的自动 Codex review 因服务额度限制未执行；PR 无 review thread，最终 diff、自审和全部必需 CI 未发现未解决的 Blocker、High 或 Medium 项。
- 2026-07-19 一次失败的 shell 故障编排在默认 `~/.local/share/muxlane` 创建了空 DB/运行目录；确认无 Account、Project、锁、事务或恢复证据后，已通过系统 Trash 做可恢复清理。

## 8. 关键工程约束

- 不把 Token、Cookie 或完整 `auth.json` 写入 SQLite、日志、RPC、诊断包或 Git。
- 锁顺序固定 Account→Project；锁文件、SQLite、heartbeat 和 GUI 状态不能替代真实 `flock`。
- Terminal transaction 终态不可改写；`credential_conflict` 必须保留证据并人工处理。
- 控制面仅 Unix Socket，不开放 LAN；WebView 不获得任意 Shell、文件或 WSL 权限。
- 不把 Phase 2/3 POC frame、Gateway 或 synthetic runner 描述为正式产品协议。

## 9. 下一步

以新的独立阶段分支启动 Phase 6；复用现有正式 control/Terminal plane，不重新实现 Phase 4/5 核心后台。

## 10. 最近核验快照

- 核验日期：2026-07-20
- 分支：`main`
- Phase 4/5 squash HEAD：`b904ef7b156fca1d059062db14a4b27513d93c9e`
- GitHub：PR #12 已合并；PR CI `29703570077` 与合并后 `main` CI `29703706489` 全绿；阶段分支本地和远端均已删除。
- 工作区与环境：收口核验时本地 `main` 与 `origin/main` 一致且工作区干净；专用 WSL、Windows checkout、staging 和 tmux test runtime 已清理。
- 关键验证：`pnpm verify`、Rust fault matrix、正式 control/Terminal integration、专用 WSL terminate、Recovery 二次 kill、systemd-nspawn boot change、Windows MSVC/Tauri 与 Windows→WSL smoke 均通过；真实 Account Usage `NOT RUN`。

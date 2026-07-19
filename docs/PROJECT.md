# Muxlane 项目真相

> 本文档是项目级长期上下文，不替代代码、Git 和运行验证。最近核验快照是时间点事实，每次任务仍须重验。

## 1. 项目定位

Muxlane 是面向 Windows 10/11 与默认 WSL2 发行版的本地 Codex Runtime 工作台。它以 Project-scoped `CODEX_HOME`、Account Vault、持久 tmux Terminal 和可恢复 Launch Transaction 为核心，不是团队账号池、自动轮换工具、云凭证服务或完整 IDE。

## 2. 核心使用场景

- 为本地 Project 注册稳定、隔离且位于 WSL 文件系统的 Runtime。
- 从用户明确选择的文件导入合成或本人持有的 Account 凭证副本，不继续引用源文件。
- 在 Account→Project 双 `flock` 下启动受管 Codex，退出后安全签回凭证。
- GUI/CLI 断开或 daemon 重启后，依据持久事务、进程身份和 Hash 重新分类。
- 通过 Windows GUI 或 `muxlane` CLI 执行 health、status、注册、启动、Recovery、Usage 探测和诊断。
- 在终端为主体的工作台中恢复 Project/Window、管理非秘密模板与命令预设、查询输入历史，并只读浏览源码。

## 3. 当前范围与非目标

当前仓库包含阶段 0 工程基础、阶段 1 冻结设计、阶段 2/3 POC、已关闭的 Phase 4/5 Runtime Control Plane，以及 Phase 6 Windows GUI 和重新划定范围后的 Phase 7 开发工作台。Phase 7 当前范围只包含模板、命令预设、输入历史、专注模式和只读文件导航。

明确延期到后续独立阶段的能力：Skills 管理、MCP 管理、Plugins 管理、统一 Asset 治理、CodeMirror、内嵌文件编辑，以及文件新增、保存、重命名和删除。Phase 8 的安装包、签名、自动更新和正式发布也不在当前范围。

## 4. 当前架构

- `muxlaned`：当前 Linux 用户范围内的单实例 Unix Socket control plane；受管 Runner 在 tmux 内独立于 GUI/daemon 生命周期。
- `muxlane`：先握手再调用 typed JSON-RPC 的稳定 JSON CLI。
- `muxlane-core`：受控目录、SQLite migration、Account/Project/Launch/Terminal/Thread/Usage 模型、双锁、Credential transaction、Incident resolution、Recovery、进程身份和诊断。
- `muxlane-protocol`：Protocol `1.0` control plane 和正式 Terminal data plane；Phase 3 POC frame 仅保留为隔离兼容测试边界。Windows host 使用 platform-neutral DTO，不链接 WSL-only core。
- SQLite 保存非秘密元数据、事务、索引和恢复审计；Filesystem 保存 Vault、Runtime、证据和日志；Linux `flock` 是活动互斥事实；tmux 是 Terminal 存活与历史载体。
- `Muxlane.exe`：React 19 + xterm.js 的终端优先工作台；Tauri Host 仅暴露编译期白名单命令，通过固定 `wsl.exe --exec /usr/bin/env muxlane|muxlaned` 适配正式协议，不接受任意 executable、Shell、tmux target 或未类型化文件操作。

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
- SQLite schema v5 的 Project settings、Project template、command preset 与 per-Project/Terminal/Thread input history；历史只保存明确提交的输入，不保存 Terminal 输出。
- 受 Project canonical root 限制的只读 workspace list/search/preview/location；拒绝 `..`、符号链接、二进制和超限文件。
- Windows 初始化/重连、Account/Usage、Project/Launch/Recovery/Incident 管理、正式 Terminal、托盘与智能关闭，以及专注/全屏工作台。

## 6. 当前阶段与路线

### 当前阶段

Phase 4/5 结论为 `PASS`。Phase 6 与 Phase 7 当前范围的实现和本地/Windows 验收已完成；阶段最终结论以本次独立 PR、review、PR CI、squash merge 和合并后 `main` CI 为关闭门禁。

### 后续粗粒度路线

1. 完成本次 Phase 6/7 PR 与合并后 `main` CI 收口。
2. 只有 Phase 6/7 均关闭后才允许以独立任务进入 Phase 8。
3. 延期的 Skills/MCP/Plugins/Asset/CodeMirror/文件编辑能力不得在 Phase 8 中顺带实现，须重新规划范围与安全模型。

## 7. 已知限制、风险与技术债

- WSL2 所有发行版共享 utility VM kernel；对专用发行版执行真实 `wsl --terminate` 不改变 kernel `boot_id`。因此 terminate 恢复与 boot identity change 分别验证：专用 WSL 执行真实 terminate，隔离 systemd-nspawn 启动提供两个真实且不同的 Linux boot identity。
- 真实账号 Usage success smoke 未获授权，状态为 `NOT RUN`；没有读取或复制全局 `~/.codex/auth.json`。fixture/fake App Server 的 schema、5h/周窗口、Reset Credit、Token、失败与并发路径已覆盖。
- Linux Desktop Rust 在当前 WSL 因缺 `pkg-config`/GTK 开发包仍为环境 `BLOCKED`；Windows MSVC Desktop check、Clippy、test、release build 和 native run 已通过，因此不构成 Windows 目标门禁失败。
- `cargo audit` 无阻断漏洞，但仓库 allowlist 仍报告 17 条未维护/unsound 警告；包括 Tauri Linux GTK3 依赖的 `RUSTSEC-2024-0429`。
- PR #12 的自动 Codex review 因服务额度限制未执行；PR 无 review thread，最终 diff、自审和全部必需 CI 未发现未解决的 Blocker、High 或 Medium 项。
- 2026-07-19 一次失败的 shell 故障编排在默认 `~/.local/share/muxlane` 创建了空 DB/运行目录；确认无 Account、Project、锁、事务或恢复证据后，已通过系统 Trash 做可恢复清理。
- 前端生产 bundle 当前约 660 kB，Vite 报告超过 500 kB 的性能警告；不影响正确性门禁，后续可按管理抽屉拆分懒加载 chunk。
- 真实账号导入、真实 Usage success 与真实 Codex 登录仍未获授权，状态为 `NOT RUN`；Windows 原生 GUI/Terminal/托盘验收使用隔离 synthetic credential 与不可认证 Codex fixture，不能解释为真实账号验收。

## 8. 关键工程约束

- 不把 Token、Cookie 或完整 `auth.json` 写入 SQLite、日志、RPC、诊断包或 Git。
- 锁顺序固定 Account→Project；锁文件、SQLite、heartbeat 和 GUI 状态不能替代真实 `flock`。
- Terminal transaction 终态不可改写；`credential_conflict` 必须保留证据并人工处理。
- 控制面仅 Unix Socket，不开放 LAN；WebView 不获得任意 Shell、文件或 WSL 权限。
- 不把 Phase 2/3 POC frame、Gateway 或 synthetic runner 描述为正式产品协议。

## 9. 下一步

完成 PR/CI/合并清理后，评估 Phase 8 的安装、签名、更新、正式发布、安全和性能范围；不得把延期的 Phase 7 能力自动并入 Phase 8。

## 10. 最近核验快照

- 核验日期：2026-07-20
- 实现分支：`feat/phase-6-7-desktop-workbench`，起点 `c31f9abf8a2468c6e1f3747067e9942d1ce58a30`。
- 关键验证：`pnpm verify`、Windows MSVC Desktop check/Clippy/test、Tauri production build、真实 Windows WebView 运行、Protocol 1.0 握手、正式 Terminal Gateway、Unicode 输入、Prompt/Shell 历史、`Ctrl+R`、专注/全屏、只读预览、托盘隐藏/恢复与 GUI 退出后任务存活均通过。
- 测试边界：Windows 原生业务链使用隔离 `/tmp` 数据根、synthetic credential 和不可认证 fixture；真实 Account Usage `NOT RUN`；Linux Desktop 因缺 GTK3 系统依赖为环境 `BLOCKED`。

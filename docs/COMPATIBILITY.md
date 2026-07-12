# Muxlane 兼容策略与验证矩阵

## 1. 文档状态与兼容原则

| 项目   | 内容                                                                     |
| ------ | ------------------------------------------------------------------------ |
| 状态   | Frozen（阶段 1）                                                         |
| 范围   | 平台支持、能力探测、协议/数据库演进与阶段 2–8 验证矩阵                   |
| 非目标 | 本轮不升级依赖、不承诺未经 POC 验证的 Codex App Server Schema 或桥接实现 |

兼容原则：

1. 只宣称实际测试过的平台；支持目标和已验证环境必须分开标注。
2. Capability probing 优先于版本猜测；版本只是诊断和候选缓存键。
3. 目标窗口是最近 2–3 个经验证的稳定 Codex CLI 版本；预发布版本不作为默认基线。
4. 不永久硬编码 Codex App Server 字段、方法或人类可读 CLI 输出；未验证内容必须标记 Candidate。
5. 新增或升级依赖时从官方 Registry、仓库或 Release 核验最新稳定版本，固定 manifest 版本并提交锁文件；本轮没有任何依赖变更。
6. 兼容阻塞必须记录原因、环境和跟踪项；不能以静默继续或假定降级掩盖问题。

## 2. MVP 平台范围

MVP 支持目标是 Windows 10、Windows 11、WSL2、默认 Ubuntu WSL 发行版和 Windows x64。暂不支持：WSL1、多 WSL 发行版管理、macOS GUI、原生 Linux GUI、Windows ARM、远程 Daemon、LAN 接入、容器内 Daemon 的正式部署及团队多用户服务。

这是一份产品范围声明，不表示阶段 1C 已在每个版本/发行版上完成验收。

## 3. 支持等级

| 等级         | 定义                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| Verified     | 在当前受支持版本、目标场景和匹配 CI/真实环境中通过了记录的验证。       |
| Supported    | 产品承诺的目标组合；实现后需持续验证，但本轮或当前环境不一定已经验证。 |
| Best Effort  | 可尝试但不作为发布保证，问题可能只提供有限帮助。                       |
| Experimental | 仅 POC 或显式实验开关可用，接口和行为可变。                            |
| Unsupported  | 不测试、不承诺，也不应通过未声明的路径“恰好可用”。                     |

“Verified”不因能编译、能打开页面、版本看起来相近或存在 CI 配置而自动获得。

## 4. 当前基线与组件兼容矩阵

以下阶段 0 工具版本是历史验证基线，不是“当前官方最新版本”的声明。本轮只读检查的本地 Codex CLI 是 `0.144.1`；它不表示当前官方最新，也不冻结 App Server Schema。

| 组件      | 阶段 0 已知基线 / 目标          | 本里程碑等级                                | 兼容要求与限制                                                        |
| --------- | ------------------------------- | ------------------------------------------- | --------------------------------------------------------------------- |
| Windows   | Windows 10 / Windows 11         | Supported                                   | Windows x64 是目标；Windows 原生 Tauri 验收由 Windows CI/环境完成。   |
| WSL       | WSL2，默认发行版                | Supported                                   | WSL1 Unsupported；多发行版管理不在 MVP。                              |
| Ubuntu    | 默认 Ubuntu WSL                 | Supported                                   | 具体 Ubuntu release 与系统包组合需要阶段 2–8 矩阵验证。               |
| Codex CLI | 本地历史检查 `0.144.1`          | Experimental adapter target                 | 近期 2–3 个经验证稳定版本是目标窗口；能力以 probe 为准。              |
| tmux      | 无冻结最低版本                  | Experimental                                | 先探测安装、版本、Control Mode 与 Socket 行为；不凭记忆写死最低版本。 |
| SQLite    | 由 Rust 依赖/目标系统提供       | Supported design target                     | 迁移、WAL/journal mode、损坏与磁盘满需要 POC。                        |
| WebView2  | Windows WebView2 可用           | Supported                                   | 安装/可用性、CSP、窗口语义需 Windows 验收。                           |
| Tauri     | `2.11.5` manifest 基线          | Supported design target                     | WSL 缺 GTK/WebKit/pkg-config 不能证明 Windows 构建失败。              |
| Node      | `22.22.2`                       | Verified only where phase 0 checks recorded | 由仓库 engines 固定；本轮不升级。                                     |
| pnpm      | `10.16.1`                       | Verified only where phase 0 checks recorded | 锁文件必须与 manifest 配对提交。                                      |
| Rust      | `rustc 1.97.0` / Cargo `1.97.0` | Verified only where phase 0 checks recorded | 桌面 crate 仍依赖目标系统库。                                         |
| Cargo     | `1.97.0`                        | Verified only where phase 0 checks recorded | 不等价于 Windows 安装包验证。                                         |

## 5. Codex CLI 能力探测

Daemon 启动或显式诊断时应以无副作用方式检查：

1. `codex` executable 是否存在和可执行；读取 version。
2. 受支持的命令与帮助文本是否表明所需入口存在。
3. App Server 是否可用，以及其 Schema/capability 是否有官方或无副作用读取方式。
4. 文件凭证模式是否受支持；不得读取、打印或传输真实 `auth.json` 以完成探测。
5. account read、rate limit/usage、session/runtime 行为和退出行为是否可经受支持能力验证。

`account/read`、`account/rateLimits/read`、`account/usage/read` 是当前设计关注的 **Candidate capability names**，不是永久上游字段。实际名称、params、result 和错误分类必须以当前安装版本的官方 Schema 或无副作用 capability probe 为准；不能把模型记忆、实验性接口或人类可读输出解析永久化。

本轮本地 `codex --help` 只确认 CLI 存在 `app-server`（标记为 experimental）命令；未运行登录、登出、刷新、App Server、Schema 查询，也未读取 Account、Session 或凭证。

## 6. 兼容适配层

`CodexAdapter` 是 Daemon 内部边界，而非 GUI 直接调用的 API。它负责：

- capability-based branching，而非单纯版本分支；
- 以版本、已探测 capability、平台和时间记录安全的 capability cache；
- allowlist 归一化可显示字段与未知字段；
- 缺少能力时返回明确降级/错误，而不是伪造数据；
- 不把未验证人类可读 CLI 输出作为长期 API；
- 不把原始敏感响应、Token、完整 `auth.json` 或未脱敏错误传给 WebView。

未知上游字段默认忽略或隔离保存为不可显示的 adapter 诊断信息；只有经审查的字段可进入 `UsageSnapshot` 或控制 RPC。上游能力的缺失可导致只读诊断、功能降级或拒绝写操作，取决于安全影响。

## 7. 协议兼容

GUI、Daemon 和 CLI 通过 [Protocol v1 Candidate](PROTOCOL.md) 的握手协商 Major/Minor 范围与明确 capabilities。Major 不兼容，Minor 只允许向后兼容扩展；未知可选字段忽略，未知 enum 值安全降级。

- 兼容：允许协商后的方法和能力。
- 降级：缺失非关键能力时显示缺失状态，禁止调用该能力。
- 不兼容：禁止业务写操作；只读版本、健康和导出前置诊断可在安全时保留。
- 升级要求：明确是 GUI、Daemon 或 CLI 必须升级；不能静默继续。

纯字符串版本比较不是功能判断，也不能绕过 capability probing。

## 8. 数据库兼容

数据库使用单调 `schema_version` 与迁移 history。迁移锁、健康检查和备份必须在业务写操作前完成；失败留在诊断状态，不能静默重建或删除数据库。

| 组合                     | 行为                                                            |
| ------------------------ | --------------------------------------------------------------- |
| 旧 GUI / 新 Daemon       | 先握手；无法理解的新能力时只读诊断或拒绝该操作。                |
| 新 GUI / 旧 Daemon       | 先握手；缺能力时降级或要求升级，不尝试写新语义。                |
| 旧 Daemon / 已升级数据库 | 默认禁止业务写入；仅允许经安全证明的只读诊断。                  |
| 迁移失败                 | 保留 DB、备份、版本与脱敏错误，进入 `MIGRATION_REQUIRED`/诊断。 |

降级版本不应打开已升级 Schema 执行写操作。发布回滚边界由迁移前备份和诊断流程定义，不承诺任意版本无损降级。

## 9. Windows—WSL 兼容 POC

阶段 2 必须测试默认发行版发现、WSL 未安装/未启用、发行版停止/启动、Windows 路径到 WSL 路径转换、空格、Unicode、大小写、符号链接、网络路径、OneDrive 与 `/mnt/c` 性能；阶段 3 必须测试 Bridge 身份绑定、重连与背压；阶段 4 必须测试 WSL `boot_id` 变化与 `wsl --terminate` Recovery。每项要分别记录支持、拒绝或降级行为。

具体 Windows—WSL Bridge、端口、命名管道、身份绑定和权限模型都属于 **POC validation required**；在此之前不得以临时 TCP/LAN 接口或 WebView 直连替代设计。

## 10. tmux 兼容

实现前需要探测 `tmux` 是否安装、版本、Control Mode、session/window 操作、history limit、Socket 权限、同名非受管 Session 和重连行为。tmux Session 存在不等于 Codex 仍运行；终端历史不能替代 durable Transaction 或进程身份。未经 POC 验证前，不永久写死最低 tmux 版本。

## 11. Tauri 与 WebView2

Windows 验收需要确认 WebView2 availability、Tauri Capability/ACL、CSP、系统托盘和窗口关闭语义。WSL 本地若缺 GTK、WebKit 或 `pkg-config`，只能说明该 Linux 环境缺 Desktop Rust 依赖，不代表 Windows 目标失败；Muxlane 不因此宣称原生 Linux GUI 支持。

## 12. 阶段 2–8 测试矩阵

| 场景                           | 2               | 3                | 4                | 5              | 6          | 7            | 8        |
| ------------------------------ | --------------- | ---------------- | ---------------- | -------------- | ---------- | ------------ | -------- |
| Windows 10 / 11、Windows x64   | Runtime 路径    | Bridge/权限      | 重启恢复         | Daemon/CLI     | GUI        | 工作台       | 发布回归 |
| 2–3 Codex 稳定版本             | Runtime/refresh | Terminal adapter | exit/Recovery    | adapter        | usage      | assets       | 发布窗口 |
| WSL 冷启动 / `wsl --terminate` | 路径            | Bridge reconnect | boot_id/Recovery | 重新连接       | GUI 恢复   | 回归         | 现场诊断 |
| GUI / Daemon 重启              | —               | 重连/背压        | transaction      | protocol       | 窗口语义   | 回归         | 升级     |
| tmux reconnect                 | —               | 探测/背压        | identity         | attach/history | UI attach  | 回归         | 运维     |
| credential refresh             | checkout/commit | —                | conflict matrix  | 正式路径       | account UI | 回归         | 发布风险 |
| SQLite migration               | —               | —                | 中断模型         | 实现/迁移      | 兼容 UI    | 回归         | 升级回滚 |
| 协议不匹配/能力缺失            | —               | handshake        | Recovery API     | CLI            | GUI        | asset API    | 发布门禁 |
| 诊断模式/导出脱敏              | —               | Bridge errors    | evidence         | CLI export     | GUI export | asset errors | 现场支持 |

表中每个单元是未来验收任务，不是阶段 1C 已完成的测试。每次验证必须记录 OS/WSL/CLI/tmux/Daemon/GUI 版本、命令、结果、已知限制和敏感数据保护结论。

## 13. 依赖与安全债务

当前非阻断项：Vite 既有大 chunk 警告；WSL 本地 Desktop check 可能因 GTK/WebKit/pkg-config 系统依赖阻塞；`cargo audit` 可能报告 GTK3/GLib 上游的 unmaintained 或 unsound 警告。阶段 1 文档提交不修改上述依赖，也不把允许的 warning 永久视为无风险。

阶段 8 发布前必须重新评估依赖、许可证、已知 vulnerability、unsound 与 unmaintained 分类；记录真实 audit 输出和上游状态，而非笼统写作“无风险”。

## 14. 不兼容行为

| 条件                                   | 必须行为                                       |
| -------------------------------------- | ---------------------------------------------- |
| 无安全协议交集                         | 阻止启动业务写操作，提供最小只读诊断。         |
| 关键能力缺失                           | 降级到安全只读能力或要求升级。                 |
| 新 Schema 被旧组件打开                 | 禁止写入，要求升级/恢复或诊断。                |
| Bridge/本地身份未通过 POC 或运行时验证 | 拒绝连接或要求 POC，不得改走 LAN。             |
| Codex 上游 Schema 未确认               | 标记 Candidate，禁用依赖它的功能，不伪造兼容。 |
| 迁移、锁、凭证或进程身份不安全         | 阻止启动、进入 Recovery/诊断，不静默继续。     |

## 15. 参考

- [协议与能力协商](PROTOCOL.md)
- [逻辑数据模型与迁移边界](DATA_MODEL.md)
- [ADR-0010：Versioned Capability Negotiation](adr/0010-versioned-capability-negotiation.md)
- [ADR-0012：Forward-only Versioned Database Migrations](adr/0012-forward-only-versioned-database-migrations.md)

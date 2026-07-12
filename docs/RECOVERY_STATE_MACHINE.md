# Muxlane 持久恢复状态机

## 1. 状态、边界与术语

| 项目     | 内容                                                                                      |
| -------- | ----------------------------------------------------------------------------------------- |
| 状态     | Frozen（阶段 1）                                                                          |
| 目标     | 对中断的 Launch Transaction 作可重复、可审计且不丢失凭证的恢复决策                        |
| 真相来源 | Linux `flock`、持久事务、受控文件系统检查、进程身份、tmux 身份和 Hash；任一单独来源均不足 |
| 非目标   | 本轮不定义 SQLite schema、Rust 类型、恢复命令 wire format 或自动解决凭证冲突              |

本状态机仅用于受管 Launch。它不为外部手工启动的 Codex、未知 tmux Session 或不属于当前 Linux 用户的数据目录背书。状态字段不得包含 Token、原始 `auth.json`、Prompt、终端输出或未脱敏错误。

## 2. 状态定义

“锁不变量”指正常运行时的要求；Daemon/WSL 崩溃后内核可能已释放 `flock`，恢复器必须重新获得锁或报告活动冲突，不能用磁盘锁文件是否存在替代检查。每次进入非终态、每次实质恢复决定和每个终态都必须耐久化。

| 状态                  | 含义与进入条件                                                              | 磁盘/锁不变量                                                                             | 允许事件与合法后继                                                                             | 崩溃后恢复                                                                           | 终态                               |
| --------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------- |
| `preparing`           | durable transaction 已创建，尚未确认 Runtime 最终凭证。                     | Vault 原件不应被修改；仅受控临时文件可存在；正常时双锁已持有。                            | checkout 成功 → `checked_out`；安全无副作用清理 → `recovered`；无法判定 → `failed`。           | 检查临时文件、Runtime、Vault Hash；不把临时内容签回。                                | 否                                 |
| `checked_out`         | Runtime `auth.json` 已原子就位并验证，但没有可信 `running` 记录。           | Runtime 常规 `auth.json` 存在且为 `0600`；正常时双锁持有。                                | 进程身份持久化 → `running`；无进程的安全收尾 → `committing_auth`；不确定 → `failed`/冲突。     | 先排除实际活进程；再按 Hash 矩阵签回、清理或冲突。                                   | 否                                 |
| `running`             | Runner/Codex 身份已记录，Runtime 凭证可能被刷新。                           | Runtime `auth.json` 存在且事务有 boot_id/PID/start ticks/identity；正常时双锁持有。       | 确认 Codex 退出 → `codex_exited`；确认仍运行 → 幂等留在 `running`；身份不明 → `failed`。       | 以锁、boot_id、PID、start ticks、cmdline、heartbeat 顺序重验；不得因心跳过期杀进程。 | 否                                 |
| `codex_exited`        | 已观察并验证 Codex 退出，尚未开始写 Vault。                                 | Runtime 凭证可能存在；正常时双锁仍持有。                                                  | 开始比较/签回 → `committing_auth`；缺失或损坏判定失败 → `failed`。                             | 确认没有原进程后进入相同 commit 决策。                                               | 否                                 |
| `committing_auth`     | 正在比较 Hash 并准备或执行 Vault 原子签回。                                 | Vault、Runtime、同目录临时或隔离副本可能并存；双锁在正常路径仍持有。                      | Vault 成功耐久化 → `auth_committed`；双更新 → `credential_conflict`；I/O/验证失败 → `failed`。 | 重新读取每个候选副本和 Hash；绝不以“最后写入”覆盖。                                  | 否                                 |
| `auth_committed`      | Vault 新版本已验证并耐久化，尚未删除 Runtime 副本。                         | Vault Hash 必须匹配该 transaction 的已提交目标；Runtime 可能仍存在；双锁仍持有。          | Runtime 清理完成 → `cleaned`；无法验证/删除 → `failed`。                                       | 仅在 Vault 目标 Hash 已证实时清理 Runtime；否则不伪造成功。                          | 否                                 |
| `cleaned`             | Runtime 活动凭证和受控临时文件已清理，凭证事务已安全收口。                  | 没有 Runtime `auth.json`；Vault 未被无证据覆盖；锁可能正在按顺序释放。                    | 正常最终记录 → `finished`；崩溃恢复收尾 → `recovered`。                                        | 验证无活进程和无活动凭证后幂等标记 `recovered`。                                     | 否                                 |
| `finished`            | 正常路径已完成，锁已释放并记录最终结果。                                    | 无活动 Runtime 凭证，无待签回事务。                                                       | 仅只读/幂等读取；拒绝状态改写。                                                                | 不执行凭证动作。                                                                     | 是，正常终态                       |
| `recovered`           | Recovery 已完成且没有凭证冲突；它只说明本事务安全收口。                     | 无待签回凭证；保留脱敏恢复审计。                                                          | 仅只读/幂等读取；拒绝状态改写。                                                                | 不执行凭证动作。                                                                     | 是，恢复终态；**不**表示冲突已解决 |
| `credential_conflict` | Vault 和 Runtime 都可能为合法但不同的新凭证，或等价证据不足以安全选择。     | 保留 Vault 当前版本、Runtime 遗留版本、签出前备份或至少 Hash/隔离副本；新 Launch 被阻断。 | 仅人工处理后的审计关闭；没有自动后继。                                                         | 保持副本并通知 CLI/GUI；不删除“多余”版本。                                           | 是，人工处理终态                   |
| `failed`              | 自动恢复无法安全推进，例如损坏文件、权限异常、身份不明或不可恢复 I/O 错误。 | 保留足以诊断的脱敏事务与安全副本；关联 Account/Project 由未解决 RecoveryIncident 阻断。   | 没有 transaction 后继；人工修复只能新建关联 RecoveryAttempt/Incident 或新 Launch。             | 不无限重试；记录错误类别和次数。                                                     | 是，不可变审计终态                 |

非法转换必须被拒绝并记录脱敏诊断；它们不能通过直接更新数据库状态绕过。重复接收当前状态的同一事件是幂等重入，不得重新覆盖 Vault、重复删除唯一副本或重启未知进程。

## 3. 完整转换表

| From              | 事件/守卫                                         | 动作                                           | To                                  |
| ----------------- | ------------------------------------------------- | ---------------------------------------------- | ----------------------------------- |
| 无                | 双锁已获、事务记录成功                            | 写入最小事务元数据                             | `preparing`                         |
| `preparing`       | checkout 原子完成且 Runtime 验证                  | 写入 checkout Hash 和状态                      | `checked_out`                       |
| `preparing`       | 未生成最终 Runtime 文件，安全清理                 | 删除受控临时文件、保留审计                     | `recovered`                         |
| `preparing`       | 最终文件/临时文件无法安全判定                     | 隔离可疑文件、记录错误                         | `failed`                            |
| `checked_out`     | Runner/Codex 身份完整且状态持久化                 | 写 PID、boot_id、start ticks、identity         | `running`                           |
| `checked_out`     | 无原进程，Runtime 文件完整                        | 进入 Hash/commit 决策                          | `committing_auth`                   |
| `checked_out`     | 文件损坏、身份冲突或不安全路径                    | 保留副本、记录原因                             | `failed` 或 `credential_conflict`   |
| `running`         | wait 事件与身份证明确认退出                       | 持久化退出观察                                 | `codex_exited`                      |
| `running`         | 同一 boot_id、PID、ticks、identity 均匹配         | 更新仅健康/恢复审计，不重启                    | `running`                           |
| `running`         | boot_id 变化且无可确认原进程                      | 停止把 PID 当活动；重估 Runtime Hash           | `codex_exited` 或 `committing_auth` |
| `running`         | 身份无法确认                                      | 不杀进程、不签回，记录人工原因                 | `failed`                            |
| `codex_exited`    | Runtime 文件可验证                                | 记录 commit intent                             | `committing_auth`                   |
| `codex_exited`    | Runtime 缺失/损坏且无安全结论                     | 隔离并记录                                     | `failed`                            |
| `committing_auth` | Hash 决策允许 Vault 原子写入且目录持久化          | 记录已提交 Vault Hash                          | `auth_committed`                    |
| `committing_auth` | Vault 与 Runtime 都变化                           | 保存所有关键副本，禁止覆盖                     | `credential_conflict`               |
| `committing_auth` | Hash/权限/I/O/目录持久化失败                      | 保留诊断和副本                                 | `failed`                            |
| `auth_committed`  | Runtime 凭证和临时文件已清理                      | 持久化清理证据                                 | `cleaned`                           |
| `auth_committed`  | Vault 目标不可验证或 Runtime 清理不安全           | 不继续删除                                     | `failed`                            |
| `cleaned`         | 正常路径释放 Project 再 Account Lock 后写最终记录 | 记录正常完成                                   | `finished`                          |
| `cleaned`         | 崩溃后恢复确认安全收口                            | 记录恢复时间和尝试次数                         | `recovered`                         |
| `failed`          | 人工修复后显式 `recover` 有安全证据               | 新建关联 RecoveryAttempt；不改写旧 transaction | `failed`（旧记录保持不变）          |
| 任意状态          | 同一 transaction_id 的同一已完成事件              | 验证前置 Hash/状态，执行零或一次操作           | 原状态或已到达后继                  |

不允许的例子包括：`running → finished`、`checked_out → finished`、`credential_conflict → auth_committed`、`failed → running`、`failed → recovered`、`failed → credential_conflict`、`finished → 任意非终态`。新 Launch 必须创建新的 transaction，不得复活旧终态事务。人工 Recovery 的 outcome 属于新建关联记录，不能伪造旧 transaction 的终态；只有新的 RecoveryAttempt 已验证无活动锁/身份/凭证风险并将关联 RecoveryIncident 标为 `resolved`，才解除启动阻断。

## 4. 持久事务概念字段

这只是逻辑模型，阶段 1B 不创建 SQLite schema 或迁移。所有时间字段使用可审计格式；错误文本是脱敏摘要而非原始 I/O、路径或凭证内容。

| 字段                                                  | 用途                                                       |
| ----------------------------------------------------- | ---------------------------------------------------------- |
| `transaction_id`                                      | 稳定、不可预测的 Launch 身份与幂等键                       |
| `project_id` / `account_id`                           | 关联稳定注册实体，不保存展示路径或秘密                     |
| `state`                                               | 上节冻结的状态枚举                                         |
| `runner_pid` / `codex_pid`                            | 候选 PID，永不单独作为进程身份                             |
| `boot_id`                                             | 创建/最后确认时的 Linux boot identity                      |
| `process_start_ticks`                                 | 从 `/proc/<pid>/stat` 取得的 starttime                     |
| `process_identity`                                    | 可验证的受管 cmdline/Runner/Codex 标识摘要，不记录敏感参数 |
| `vault_hash_before_checkout`                          | 签出前 Vault 内容 Hash                                     |
| `runtime_hash_at_checkout`                            | checkout 完成后的 Runtime Hash                             |
| `runtime_hash_at_recovery` / `vault_hash_at_recovery` | Recovery 决策时重新计算的 Hash                             |
| `credential_backup_reference`                         | 受控、权限受限的冲突/备份位置相对引用；不能是任意用户路径  |
| `created_at` / `updated_at`                           | 创建与最后耐久化变更时间                                   |
| `last_error_code` / `last_error_message_redacted`     | 稳定分类与脱敏摘要                                         |
| `recovery_attempts` / `recovered_at`                  | 自动恢复摘要；人工恢复的逐次审计在独立 RecoveryAttempt 中  |
| `schema_version`                                      | 未来持久化格式兼容闸门                                     |

## 5. 进程身份验证

恢复器按以下优先级做决定：

1. 当前真实的 **`flock` 状态**：活动锁冲突优先于陈旧数据库记录，但锁本身不证明进程是哪个程序。
2. Linux **`boot_id`**：与事务不同表示 Linux/WSL 已重启，旧 PID 不得再解释为原进程。内核将 `boot_id` 记录为启动后保持不变的 UUID。[Linux kernel documentation](https://docs.kernel.org/admin-guide/sysctl/kernel.html)
3. **PID**：只作为到 `/proc` 的候选入口，单独不可靠。
4. **`/proc/<pid>/stat` start ticks**：字段 22 是进程自系统启动以来的启动时刻（clock ticks），与记录值不匹配即拒绝认定为原进程。[proc_pid_stat(5)](https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html)
5. **cmdline 或可验证的 process identity**：必须符合受管 Runner/Codex 的预期标识；不能将含秘密的完整命令行写入事务或日志。
6. **heartbeat**：只显示健康信息，不作为死亡、锁释放、签回或 kill 的判据。

不能确认身份时：不得杀进程、不得把它附加为受管 Session、不得签回可能仍被该进程修改的 Runtime 凭证；进入 `failed` 并要求人工诊断。

## 6. Daemon 启动恢复流程

1. 初始化受控数据目录，验证所有权、模式和无非预期符号链接。
2. 验证 SQLite/事务存储版本可读；无法安全读时保留原文件并进入人工恢复。
3. 扫描所有非终态 transaction，并列出终态 transaction 的未解决 RecoveryIncident；`failed` 只供诊断或显式人工尝试，不自动改写。
4. 扫描受管 tmux Session，并区分受管身份和仅同名的未知 Session。
5. 以固定 **Account → Project** 顺序尝试检查/获得相关锁；真实活动锁禁止抢占。
6. 读取当前 Linux `boot_id`。
7. 对记录 PID 读取 start ticks 和 process identity，绝不只检查 PID 存在。
8. 检查 Runtime `auth.json` 的常规文件类型、权限、受控路径和存在性。
9. 计算 Runtime 与 Vault Hash，读取交易的签出前/checkout Hash。
10. 将事务分类为：原 Codex 仍运行、可恢复监督、可自动 commit、可安全清理、`credential_conflict` 或 `failed`。
11. 只执行与该分类一致的幂等动作；每一次文件改变重新持久化事务。
12. 对非终态 transaction 记录 `recovered`、`credential_conflict` 或 `failed`；对已终态的人工操作追加 RecoveryAttempt，包括尝试次数和脱敏错误类别。只有该 Attempt 的安全证据满足启动前置条件时，才将关联 RecoveryIncident 标为 `resolved`；旧 Transaction 终态保持不变。
13. 通过 GUI/CLI 的状态/诊断接口展示结果与下一步，不展示 Token 或原始凭证。

## 7. Hash 冲突决策矩阵

| 情形                                  | 证据与决策                                                                                                                 | 必须保留/禁止动作                                                                                                                                                                              |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A. Vault 未变化，Runtime 有更新       | `vault_hash_at_recovery == vault_hash_before_checkout` 且 Runtime 与 checkout Hash 不同且完整；通常允许 Runtime 原子签回。 | 先保留签出前备份或可审计 Hash；成功后进入 `auth_committed`。                                                                                                                                   |
| B. Vault 未变化，Runtime 与原签出一致 | 两者没有新凭证更新；允许清理或完成事务。                                                                                   | 不必无意义重写 Vault；保留事务审计后 `cleaned`。                                                                                                                                               |
| C. Vault 已变化，Runtime 未变化       | Runtime 等于 checkout Hash，而 Vault 与签出前不同；不得覆盖新的 Vault。                                                    | 仅在确认原 Codex/Runner 已不存在、完成 boot_id 与进程身份检查、Runtime Hash 等于签出基线、确认不存在 Runtime 更新、保留审计记录且清理可幂等时，才可清理遗留 Runtime；不得声称 Runtime 已签回。 |
| D. Vault 已变化，Runtime 也变化       | 两个版本均可能是合法 refresh，且与签出前不同。                                                                             | 进入 `credential_conflict`；保留当前 Vault、Runtime 遗留凭证和签出前备份或 Hash；绝不自动选胜者。                                                                                              |
| E. Runtime 缺失                       | 结合状态、Vault Hash、进程身份判断：运行身份可信时不能假定退出；无进程且 Vault 无需写入时才可清理。                        | 不得伪造签回成功；证据不足为 `failed`。                                                                                                                                                        |
| F. Runtime `auth.json` 损坏           | 文件类型、权限、内容完整性或 Hash 计算失败。                                                                               | 隔离损坏文件并保留 Vault；绝不写入 Vault；进入 `failed` 或需人工处理的冲突。                                                                                                                   |

Hash 是冲突检测证据，不是凭证正确性的排序依据。任何人工处理应要求重新登录或明确用户选择，并形成新的 RecoveryAttempt/Incident 审计动作；不修改已终态 transaction 的历史结论。

## 8. 原子文件操作不变量

1. 临时文件与目标文件必须在同一受控目录和同一挂载文件系统；跨文件系统 `rename` 不可假设原子，可能返回 `EXDEV`。[rename(2)](https://man7.org/linux/man-pages/man2/rename.2.html)
2. 创建时先设为 `0600`，验证常规文件类型、目标父目录、所有权和无非预期符号链接，然后写入并验证完整性。
3. 对临时文件 `fsync`，再执行同目录原子 rename，最后 `fsync` 父目录。Linux 文档明确文件 `fsync` 不保证包含该文件目录项的持久化。[fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html)
4. 所有路径操作应以受控目录描述符和 no-follow 策略消除可利用的路径替换；不得先检查后用普通字符串路径打开敏感文件。
5. 失败必须保留可诊断的状态和足以恢复的受控副本；敏感临时文件仅在确认存在另一可信副本后受控清理。
6. 所有这些语义必须在实际 WSL 本地文件系统与断电/中断故障注入中验证，不能把文档中的流程当成平台无条件保证。

## 9. 锁恢复与 tmux/进程分类

### 9.1 锁恢复

- `flock` 随持有它的进程和相关打开文件描述符关闭而由内核释放；磁盘锁文件仍存在不意味着锁仍被持有。
- SQLite 占用记录、GUI 显示和 heartbeat 可能滞后，只能辅助诊断。
- 恢复器协调真实 `flock`、事务、boot_id/进程身份、Runtime Hash 和 tmux；不得仅删除锁文件“解锁”。
- 有真实活动锁且身份/事务不能安全关联时，禁止静默抢占；向 CLI/GUI 报告冲突和人工入口。

### 9.2 tmux 与进程恢复分类

| 观察结果                                     | 处理                                                                         |
| -------------------------------------------- | ---------------------------------------------------------------------------- |
| Codex 仍运行，tmux 存在                      | 身份完整时恢复监督/GUI 重新附加；不进行 commit。                             |
| Codex 仍运行，GUI 不存在                     | 保持运行，允许 GUI/CLI 重新附加；GUI 不影响 transaction。                    |
| Runner 不存在，Codex 存在                    | 仅身份验证成功才重新建立监督；不杀、不重启 Codex。                           |
| tmux 存在，Codex 不存在                      | tmux 不证明运行；若事务/Hash 证明退出，进入 commit 或安全清理。              |
| transaction 为 `running`，进程不存在         | 重新检查 boot_id/identity；无可信活动进程时进入 `codex_exited`/commit 决策。 |
| transaction 为 `checked_out`，Codex 从未启动 | 按 Hash 矩阵 commit、清理或冲突；不得把它标为正常 `finished`。               |
| WSL boot_id 已改变                           | 所有旧 PID 无效；扫描 Runtime/Hash，禁止对同 PID 新进程操作。                |
| 未知非受管 tmux Session 同名                 | 禁止附加、杀死或复用；报告命名冲突，人工处理或使用受管 identity。            |
| 进程身份无法确认                             | 禁止 kill、附加、签回和自动解锁；进入 `failed`。                             |

## 10. 重试、幂等与故障注入

非终态 `transaction_id` 的自动 Recovery 可以重复执行。每次写入 Vault 前都重新验证状态/Hash；每次删除 Runtime 前都确认 Vault 目标副本存在；恢复尝试递增并保留脱敏错误类别。达到实现时冻结的有限上限后，必须进入不可变 `failed` 人工处理，而不是无限循环。此后每次人工动作新建 RecoveryAttempt；重试不能重复覆盖 Vault、重复删除唯一凭证副本或把 `credential_conflict` 当作已解决。

| 故障注入                              | 预期终态/下一状态                        | 验收不变量                               |
| ------------------------------------- | ---------------------------------------- | ---------------------------------------- |
| `/exit`、EOF、正常退出                | `finished`                               | Runtime 凭证签回后才删除并释放锁。       |
| Ctrl+C                                | 等待实际退出；随后 `codex_exited`        | 发送中断不等于退出。                     |
| 关闭终端、关闭 GUI                    | 继续 `running` 或可重连                  | GUI/Client 事件不改事务。                |
| 杀死 Codex                            | `codex_exited` 或 Recovery               | 先确认身份；不直接标 finished。          |
| 杀死 Runner                           | Recovery 分类                            | 不假定 Codex 也退出。                    |
| 杀死 Daemon                           | 启动后 `running`/commit/`recovered`/冲突 | 不丢凭证，不仅依赖陈旧 SQLite。          |
| `wsl --terminate` 或 Windows 重启模拟 | Recovery，boot_id 改变                   | 旧 PID 永不可信，不能误杀同 PID 新进程。 |
| checkout 写入中断                     | `recovered` 或 `failed`                  | 部分临时文件绝不写回 Vault。             |
| commit 写入中断                       | `auth_committed`、冲突或 `failed`        | 不覆盖未知较新 Vault，副本可诊断。       |
| rename 后、目录 fsync 前中断          | Recovery                                 | 不把路径存在误作持久化成功。             |
| `auth.json` 损坏                      | `failed`                                 | 隔离 Runtime，Vault 不被写入。           |
| Vault Hash 冲突                       | `credential_conflict`                    | Vault/Runtime/签出前证据均保留。         |
| 同 Account 并发申请                   | 第二请求拒绝                             | 不自动切账号，不抢占 Account Lock。      |
| 同 Project 重复启动                   | 第二请求拒绝                             | 不产生第二受管主实例。                   |
| tmux 重新附加                         | `running` 或 CLI/GUI 重连                | Client 与 Session 生命周期不混淆。       |
| SQLite 迁移失败                       | `failed`/维护恢复                        | 不部分升级、不删除可恢复旧数据。         |
| 软链接攻击、路径替换                  | `failed`                                 | 不跟随敏感目标，Vault 不泄露或被覆盖。   |
| 磁盘空间不足、`fsync` 失败            | `failed` 或冲突                          | 未验证写入不标成功，保留可恢复副本。     |
| 权限改变                              | `failed`                                 | 拒绝继续凭证操作，报告脱敏权限类别。     |

## 11. 实现前 POC 清单

阶段 2 POC 必须验证同目录 rename/目录 `fsync` 故障窗口、目录 fd/no-follow API、当前 Codex CLI refresh 行为和 Account 接管；阶段 3 POC 必须验证 tmux 受管标识；阶段 4 POC 必须验证 Linux/WSL 的 `flock` 释放、真实 Runner/Codex process identity、WSL 重启 boot_id、SQLite 中断恢复及全部故障注入。任何结果若与本状态机假设矛盾，必须先以 ADR 与本文件修订收口，再实现正式业务代码。

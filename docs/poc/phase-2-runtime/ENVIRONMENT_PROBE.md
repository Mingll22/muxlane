# 阶段 2A 环境与 Codex CLI 探测报告

> 本报告的可提交版本只记录脱敏结论。原始命令输出只允许存放在当前机器的 `<POC_ROOT>/evidence/environment-probe.txt`（`0600`），不得提交。这里的 `<USER_HOME>`、`<REPO_ROOT>`、`<POC_ROOT>` 与 `<CODEX_BINARY>` 均为占位符。

## 执行方式

```bash
export POC_ROOT="${XDG_STATE_HOME:-$HOME/.local/state}/muxlane-poc/phase-2-runtime"
bash poc/phase-2-runtime/scripts/init-poc-root.sh --poc-root "$POC_ROOT"
bash poc/phase-2-runtime/scripts/probe-environment.sh --poc-root "$POC_ROOT"
```

探测会记录操作系统/WSL、发行版、内核、CPU 架构、Shell、`HOME`、XDG 目录、umask、仓库与 POC 根目录的文件系统、Rust/Cargo/Node/pnpm、Codex CLI 路径/版本/安全可判断的安装来源，以及观察到的帮助命令。它不主动把现有全局 Codex 目录作为输入，也不导出任何凭证文件内容；没有 `strace` 或等价证据时，不能由此断言 CLI 从未访问全局 Home。

## 结论分类

- **Observed**：命令真实执行并产生可复核输出。
- **Inferred**：仅根据路径、帮助文本或受限 file syscall 观察得出的解释，不能升级为运行时保证。
- **Not Verified**：未执行、命令未暴露，或该行为需要阶段 2B 的受控凭证副本。
- **Blocked**：缺少本地命令、权限或安全前置条件。

## 可填写的脱敏摘要

| 项目                         | 分类                    | 脱敏记录格式                                                  |
| ---------------------------- | ----------------------- | ------------------------------------------------------------- |
| 操作系统与 WSL               | Observed / Blocked      | Linux/WSL 识别结果；不记录用户名或主机名。                    |
| WSL 发行版与版本             | Observed / Not Verified | 发行版与 `wsl.exe --version` 是否可用。                       |
| Kernel、CPU、Shell           | Observed                | 版本与架构；Shell 仅记录名称或安全路径类别。                  |
| HOME 与 XDG                  | Observed                | 只写 `<USER_HOME>` 与相对/占位路径。                          |
| umask                        | Observed                | 八进制模式。                                                  |
| 仓库 / POC 文件系统          | Observed                | 文件系统类型及“Linux native / rejected”结论；路径使用占位符。 |
| Rust/Cargo/Node/pnpm         | Observed / Blocked      | 真实版本号。                                                  |
| Codex 路径与版本             | Observed / Blocked      | `<CODEX_BINARY>` 与真实版本号。                               |
| Codex 安装来源               | Inferred / Not Verified | 仅路径或 package metadata 可安全支持时写明。                  |
| `CODEX_HOME` 探测            | Observed / Not Verified | disposable 目录前后文件元数据；不能写成 Session/凭证隔离。    |
| `resume`、App Server、Schema | Observed / Not Verified | 仅基于实际 `--help` 或确认的无副作用 Schema 入口。            |
| Account / rate limit 能力    | Not Verified / Blocked  | 不读取真实 Account、Usage 或凭证。                            |

## 阶段 2A 解释规则

1. `codex --help` 或 `app-server --help` 成功，只证明相应帮助入口被当前 CLI 暴露；不证明 App Server Schema、常驻运行、权限或生产兼容。
2. disposable `CODEX_HOME` 的目录未变化，只能说明这两条无副作用命令未观察到目录写入；不证明真实会话、配置、历史或凭证隔离。
3. 文件元数据、Hash 或 mtime 只能说明文件发生或未发生变化。Hash 未变时，结论只能是“本次未观察到文件变化”。
4. 真实 `auth.json` 格式接受、Token refresh、Account A/B 接管、Session 连续性和 Project Runtime 隔离均保留到 2B/2C 的受控测试。

## 本次脱敏实际摘要（2026-07-12）

| 项目                       | 分类         | 结果                                                                                                                                                                                                          |
| -------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OS / WSL / Kernel / CPU    | Observed     | WSL2 Linux；`Linux 6.6.87.2-microsoft-standard-WSL2`，`x86_64`。                                                                                                                                              |
| WSL 发行版与版本           | Observed     | Ubuntu `24.04.4 LTS`；`wsl.exe --version` 输出的 WSL 版本为 `2.6.3.0`。                                                                                                                                       |
| Shell / HOME / XDG / umask | Observed     | Shell 为 Bash；`HOME` 为 `<USER_HOME>`；XDG 覆盖变量未设置；umask 为 `0077`。                                                                                                                                 |
| 仓库与 POC 文件系统        | Observed     | `<REPO_ROOT>` 与 `<POC_ROOT>` 都位于 `ext4`；POC 根目录已创建在 Linux 原生文件系统、模式为 `0700`。                                                                                                           |
| Rust / Cargo / Node / pnpm | Observed     | `rustc 1.97.0`、Cargo `1.97.0`、Node `v22.22.2`、pnpm `10.16.1`。                                                                                                                                             |
| Codex 可执行文件与版本     | Observed     | `<CODEX_BINARY>` 解析到 npm 全局 `@openai/codex` 包，版本 `0.144.1`。安装来源是基于该包路径的 Inferred 结论。                                                                                                 |
| `CODEX_HOME`               | Observed     | 帮助文本引用 `$CODEX_HOME`，且新的 disposable 目录用于 `--version` / `--help` 时出现了仅本地的临时文件；命令结束后均被清理。未证明 Session 或凭证隔离。                                                       |
| Session 恢复入口           | Observed     | `resume` 已由顶级帮助暴露；其帮助说明可选择历史交互式 Session。未执行恢复，也未读取现有 Session。                                                                                                             |
| App Server 与 Schema       | Observed     | `app-server` 为 experimental。`generate-json-schema --out <DIR>` 已在 disposable `CODEX_HOME` 中成功运行，生成 schema bundle 的元数据后立即删除；未启动服务、未保留或提交 Schema，也未将其视为 Muxlane 合同。 |
| Account / rate-limit 能力  | Inferred     | Schema bundle 的文件名显示 Account 与 rate-limit 相关描述符存在；未启动 App Server，未读取 Account、Usage、rate limit 或真实凭证，因此实际 capability 与字段均为 Not Verified。                               |
| 全局 Codex Home 未访问     | Not Verified | 当前环境没有 `strace`；无副作用命令的 disposable 测试不能证明未访问全局 Home。                                                                                                                                |

本次 raw evidence 的模式为 `0600`，Harness 的敏感标记扫描为清除，POC 根目录复核为无 `auth.json`、无符号链接。该结论只覆盖本次 Harness 输入与本地证据，不表示真实凭证安全、Token refresh 或 Runtime 生命周期已验证。

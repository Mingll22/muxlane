# 阶段 2A Harness 安全不变量

> 本说明仅适用于 `poc/phase-2-runtime/` 下的非生产 Shell Harness。它不是正式 Vault、Credential Transaction、锁或 Recovery 的实现，也不能作为其安全保证。

## 强制保护

| 保护         | Harness 行为                                                                                                                                                                                                                                  |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 显式范围     | 每个命令都要求 `--poc-root <absolute-path>`；没有默认 POC 根目录，也不会操作未传入目录。                                                                                                                                                      |
| 危险路径拒绝 | 拒绝空值、相对路径、`..`、`/`、`$HOME`、真实 `~/.codex` 及其子目录、仓库根目录及其子目录。                                                                                                                                                    |
| 文件系统边界 | 拒绝 `/mnt/*`；只接受探测到的 `ext2`/`ext3`/`ext4`/`xfs`/`btrfs`/`zfs`/`tmpfs`/`overlay` Linux 原生类型。                                                                                                                                     |
| 符号链接     | 拒绝输入路径的任何符号链接组件；验证时拒绝 POC 根目录中的任何符号链接；元数据检查特别拒绝符号链接 `auth.json`。                                                                                                                               |
| 权限/所有者  | POC 根目录及所有预定义子目录必须归当前用户所有且为 `0700`；环境 evidence 为 `0600`。                                                                                                                                                          |
| 凭证边界     | 2A 验证器拒绝所有 `auth.json`。Harness 不复制、移动、读取、重写或删除真实凭证；也不要求用户提供凭证。                                                                                                                                         |
| 输出最小化   | 文件检查只显示 `<POC_ROOT>` 占位路径、类型、权限、owner、大小、mtime、SHA-256；绝不打印文件内容。                                                                                                                                             |
| CLI 探测     | 只运行确认存在的 `--version` / `--help` 命令，以及新的 disposable `CODEX_HOME` 的同类命令；若帮助明确暴露 `generate-json-schema`，只向 disposable 输出目录生成 Schema 元数据后删除。不会登录、登出、启动交互式 Codex、App Server 或恢复会话。 |
| 证据         | 原始探测信息只在 `<POC_ROOT>/evidence/` 保存；扫描常见凭证标记，检测到时以非零退出且不回显文件内容。                                                                                                                                          |

Harness 使用 `set -Eeuo pipefail`，不使用 `set -x`、`eval` 或拼接未经验证的 Shell 命令。所有路径变量均加引号。创建目录前会打印目标路径；错误以非零状态返回。

## 目录与文件语义

`init-poc-root.sh` 只创建空布局；其幂等运行会恢复定义目录的 `0700` 权限。`verify-poc-safety.sh` 是只读验证：它检查所有定义目录、所有者、权限、符号链接和 `auth.json` 的不存在。`inspect-file-metadata.sh` 可用于 synthetic 文件，或作为后续阶段的受控元数据工具；它不适合作为阶段 2A 使用真实凭证的授权。

`probe-environment.sh` 创建一个新的空的 disposable `CODEX_HOME`，比较 `codex --version` 和 `codex --help` 前后的目录元数据；若当前帮助实际暴露 `generate-json-schema`，再将 Schema 输出到另一个 disposable 目录、记录元数据后删除两者。该检查最多说明这些探测命令的文件副作用；它不证明 Session、凭证或 Account 隔离。若系统有 `strace`，脚本仅以 file syscall 观察 `codex --version` 是否出现 `.codex` 路径；没有 `strace` 时该结论必须保留为未验证。

## 明确不保证的内容

Shell 只能在创建前后检测符号链接，不能提供阶段 4 需要的基于目录文件描述符的 no-follow 操作，因此它不宣称可消除同 UID 对手的全部 TOCTOU 竞态。Harness 不执行任何凭证写入；这个限制使该剩余风险不扩大到真实凭证。后续若进行真实 Credential Checkout / Commit，必须先完成单独的文件描述符/no-follow、同目录原子替换、文件和目录 `fsync`、双锁、事务与 Recovery POC，不能复用本 Harness 当作正式实现。

下列能力全部明确不在阶段 2A：

- Account/Project Repository、SQLite 业务表、正式 JSON-RPC、Daemon、GUI；
- Account Lock / Project Lock、Launch Transaction、Credential Checkout / Commit、自动 Vault 覆盖、Recovery；
- tmux、Terminal、App Server 常驻服务、Usage 查询、Windows—WSL Bridge；
- 真实 Token refresh、真实 Account A/B 接管、真实 Session 恢复。

这些限制对应冻结设计的核心不变量：秘密不进入日志/SQLite/诊断包；Runtime 不位于仓库或 Windows 挂载；冲突不可自动覆盖；锁与进程身份不能由本 Harness 替代。

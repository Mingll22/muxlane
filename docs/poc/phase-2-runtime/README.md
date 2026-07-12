# 阶段 2 Runtime POC

> 状态：阶段 2A 已建立非生产 Harness；阶段 2B、2C、2D 尚未执行。本文描述 POC 的安全实验边界，不代表 Account Vault、Project Runtime Manager、Credential Checkout / Commit、锁、事务、Recovery、Daemon、RPC、tmux 或 GUI 已实现。

## 范围与阶段

| 阶段 | 目标                                                                | 当前状态                 |
| ---- | ------------------------------------------------------------------- | ------------------------ |
| 2A   | 环境与 Codex CLI 无副作用探测；安全目录模型；最小 Harness；测试协议 | 本次里程碑               |
| 2B   | 单 Account A 的受控凭证副本、运行、退出和变化观察                   | 未执行；需要用户本地材料 |
| 2C   | Account A/B 对同一 Project Runtime 的顺序接管                       | 未执行；需要 2B 结论     |
| 2D   | 路径、文件系统、失败窗口与 POC 结论收口                             | 未执行                   |

阶段 2A 只允许空目录、非凭证 synthetic fixture、权限检查、文件元数据/Hash 与 `codex --version` / `codex --help` 等无副作用探测。若当前 CLI 从帮助中明确暴露 Schema 导出入口，可只向 disposable 目录导出、记录元数据后立即删除。它绝不读取、复制、移动或修改真实 `auth.json`，也不启动交互式 Codex、登录/登出、App Server 或真实会话。

## 本地运行入口

所有脚本必须显式传入一个位于 Linux 原生文件系统、且不在仓库中的 POC 根目录。根目录本身可位于 `$HOME` 的安全子目录，但不能是 `$HOME`、真实 `~/.codex` 或其子目录。初始化只接受不存在或空且已为 `0700` 的目录；初始化成功后再次运行会拒绝非空根目录，而不会改动既有 POC 或未知用户文件。结构验证可重复运行。以下变量只在本地 shell 中设置；不得把其展开后的路径、evidence 或 Runtime 文件提交到 Git。

```bash
export POC_ROOT="${XDG_STATE_HOME:-$HOME/.local/state}/muxlane-poc/phase-2-runtime"

bash poc/phase-2-runtime/scripts/init-poc-root.sh --poc-root "$POC_ROOT" --dry-run
bash poc/phase-2-runtime/scripts/init-poc-root.sh --poc-root "$POC_ROOT"
bash poc/phase-2-runtime/scripts/verify-poc-safety.sh --poc-root "$POC_ROOT"
bash poc/phase-2-runtime/scripts/probe-environment.sh --poc-root "$POC_ROOT"
```

`probe-environment.sh` 把原始探测信息只写入 `<POC_ROOT>/evidence/`，文件模式为 `0600`。所有 Codex CLI 探测都显式使用新的 disposable `CODEX_HOME`；这仍不能在没有 `strace` 或等价证据时证明全局 Home 未被访问。报告会在写入前后保持 POC 根目录 `0700`，并在结束时扫描常见凭证标记；发现标记时命令非零退出且不在终端回显证据内容。

安全的文件元数据检查示例：

```bash
printf 'synthetic, non-credential data\n' >"$POC_ROOT/tmp/synthetic.txt"
chmod 0600 "$POC_ROOT/tmp/synthetic.txt"
bash poc/phase-2-runtime/scripts/inspect-file-metadata.sh \
  --poc-root "$POC_ROOT" \
  --file "$POC_ROOT/tmp/synthetic.txt"
```

该命令只输出 `<POC_ROOT>` 占位路径、文件类型、权限、`current-user`、大小、mtime 和 SHA-256；不会输出文件内容。待检查的 synthetic 文件必须归当前用户所有且为 `0600`。阶段 2A 的安全验证会拒绝 POC 根目录内出现任何 `auth.json`，因此上例只能用于 synthetic 非凭证文件。

## 目录模型

初始化后，POC 根目录包含以下空的、模式 `0700` 的目录。它是测试隔离布局，不是正式产品的数据模型。

```text
<POC_ROOT>/
├── accounts/{account-a,account-b}/
├── projects/
│   ├── project-a/codex-home/
│   └── project-b/codex-home/
├── backups/
├── evidence/
├── manifests/
└── tmp/
```

阶段 2A 不在这些目录创建 `auth.json`。仓库中唯一的 fixture 是 `poc/phase-2-runtime/fixtures/synthetic-non-credential.json`，它明确表示没有凭证，不能用于任何登录、刷新或 Checkout 测试。

## 不得提交的本地数据

不得提交 POC Root、Vault、Runtime、backup、evidence、manifest 实例、临时文件、真实 Hash 清单、Session、日志或真实 `auth.json`。`.gitignore` 只为仓库内可能误建的阶段 2 POC 数据位置设置精确规则；正常 Harness 源码、fixture 和本文档仍受 Git 跟踪。

完整的安全不变量、已知限制和测试协议分别见：

- [Harness 安全说明](HARNESS_SECURITY.md)
- [环境探测报告](ENVIRONMENT_PROBE.md)
- [阶段 2 测试协议](TEST_PROTOCOL.md)

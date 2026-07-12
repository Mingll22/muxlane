# 安全审计

- Tauri 只注册 Phase 3 的固定 Command：probe、受管 Session/Window、attach/detach、输入、resize、close、cleanup；没有通用命令执行。
- Host 固定启动 `wsl.exe --exec muxlaned phase3 gateway --socket muxlane-p3`，没有来自 WebView 的 executable、shell 或 argv。
- Gateway 固定执行 `tmux`，使用参数数组；无 `sh -c` / `bash -c` / PowerShell 拼接。
- 无网络 listener；transport 为 Tauri IPC + stdin/stdout。tmux 仅用独立用户 socket，观察到模式为 `0660`。
- 输入通过十六进制单字节 `send-keys -H` 传输；target 由 Gateway 验证后的内部 ID 构造。
- CSP 保持 `connect-src 'self'`；Capability 只有 `core:default`，未添加 shell、filesystem、process 或 HTTP plugin permission。
- POC 不读 `auth.json`，不使用凭证、Cookie、Token、真实 prompt 或业务输出。
- React/TypeScript 审查未发现 `dangerouslySetInnerHTML`、DOM HTML sink、`eval`/`Function`、浏览器持久化、`postMessage`、动态导航或公开环境变量；不存在 Tauri shell/filesystem plugin。

已执行的等价扫描覆盖本轮 23 个改动/新增文件：私钥、危险 Unicode、本地 Home/Windows 用户绝对路径、凭证标记、通用 shell 入口和 network listener source 均为 0 匹配；`git diff --check` 通过。Gitleaks 与 ShellCheck 在环境中不可用。

`pnpm audit --prod --audit-level=high` 退出 0，未发现已知 high 级生产依赖漏洞。`cargo audit` 退出 0，但报告 17 条 transitive dependency warning，包括 GTK3 bindings 的 unmaintained advisories 和 `glib 0.18.5` 的 unsound advisory；它们来自既有 Tauri Linux dependency graph，本轮未升级或忽略。真实 Windows CI 必须复核这一结果。

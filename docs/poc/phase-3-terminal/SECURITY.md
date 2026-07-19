# 安全审计

## 能力面

- Tauri 只注册 Phase 3 命名 Command：probe、受管 Session/Window、attach/start/detach、输入、resize、close、cleanup；没有通用命令执行。
- Host 固定启动 `wsl.exe --exec /usr/bin/env muxlaned phase3 gateway --socket muxlane-p3`。WebView 不能控制 executable、argv、PATH、socket 或 WSL distribution。
- Gateway 固定执行 `tmux` 参数数组；无 `sh -c`、`bash -c`、PowerShell 或用户字符串拼接。
- Session、Window、Pane、stream token、input 与 resize 均进行长度、字符集、范围和存在性验证。
- 输入逐字节编码为 `send-keys -H`；target 只由验证后的内部 ID 构造。
- Capability 只有 `core:default`；未引入 Tauri shell、filesystem、process、HTTP 或 network plugin。
- CSP 的 `script-src`/`connect-src` 均为 `'self'`；`style-src 'unsafe-inline'` 仅支持现有组件样式，不放宽脚本执行。

## transport 与持久化

- 产品 transport 仅为 Tauri IPC + stdio；Gateway 和 release Tauri 应用无 TCP listener。
- GUI E2E 的 Vite 与 WebView2 CDP 是临时测试入口，分别只监听 `127.0.0.1:1420` / `127.0.0.1:9333`；不属于 release。
- release `Muxlane.exe`、直接子进程和 WSL Gateway 的监听端口审计为 0。
- WebView runtime 中 `localStorage=0`、`sessionStorage=0`、IndexedDB 和 Cache Storage 均为空；终端输出不写普通应用日志或浏览器持久化。
- POC 不读 `auth.json`，不使用凭证、Cookie、Token、真实 prompt、用户 Shell 或业务输出。

## 扫描结果

只读扫描覆盖所有 tracked source/docs：

- 私钥 marker：0；疑似硬编码 secret assignment：0；
- `/home/<user>` / `C:\Users\<user>` 真实用户绝对路径：0；
- BOM、文本 NUL、Unicode `Cf` 控制字符：0；
- React HTML sink、`eval`/`Function`、浏览器持久化 API：0；
- source 中 TCP/UDP listener：0；`git diff --check`：PASS。

`gitleaks` 在环境中不可用，因此没有虚构其结果；上述扫描使用 `rg`、Git tracked-file 枚举和只读字节/Unicode 检查完成。

## 依赖审计

`pnpm audit --prod --audit-level=high` 退出 0，无已知漏洞。`cargo audit` 退出 0、无可阻断 vulnerability，但报告 17 个 allowed warning，包括 Tauri Linux 依赖图中的 GTK3 bindings unmaintained 与 `glib 0.18.5` unsound advisory。本轮未新增、升级或忽略这些依赖；它们作为已知非核心限制保留。

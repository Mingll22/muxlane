# 环境与 Control Mode 探测

探测日期：2026-07-13。以下为可提交的规范化摘要；原始终端输出、Home、主机、IP、欢迎信息和用户路径均未保存。

| 项目               | 结果                                                              |
| ------------------ | ----------------------------------------------------------------- |
| Host               | Linux WSL 环境，非 Windows GUI Host                               |
| Rust               | 1.97.0                                                            |
| Node / pnpm        | 22.22.2 / 10.16.1                                                 |
| tmux               | 3.4                                                               |
| `wsl.exe`          | 可从当前环境解析                                                  |
| Control Mode       | PASS：`tmux -L <isolated> -C attach-session` 返回结构事件         |
| 专用 socket        | PASS：socket 为用户所属的 `0660`；父目录为系统 `/tmp` sticky 目录 |
| Windows Tauri Host | NOT VERIFIED：当前不是 Windows Host                               |

## 观察到的事件语义

在独立 socket 与 synthetic Session 上，Control Mode 返回 `%begin` / `%end` 命令边界、`%session-changed`、`%output <pane> <octal-escaped-bytes>`、`%window-add`、`%window-renamed`、`%layout-change` 与 `%unlinked-window-close`。`%output` 以 tmux 的八进制转义承载原始终端字节；Gateway 将其还原为字节数组，而不假定 UTF-8。

创建、rename、kill Window 和 `resize-window -x 101 -y 31` 已在隔离 server 验证。tmux 的显示 ID 为 Session `$N`、Window `@N`；POC 只接受 Gateway 自己列表后验证过的 `@<digits>`，不会接收 WebView 传来的任意 tmux target。

## 探测安全

探测曾发现默认 interactive Shell 会产生系统欢迎信息，因此该内容没有被记录，后续 POC 不启动默认 Shell。`muxlaned phase3 synthetic-runner` 是固定可执行文件，输出只含 synthetic ANSI、中文、宽字符、Emoji 和计数器。

探测结束调用 `tmux -L <isolated> kill-server`；未连接、枚举或改动默认 tmux server。

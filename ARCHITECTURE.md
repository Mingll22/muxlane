# 阶段 0 架构边界

本文描述当前仓库的工程边界，不是最终系统架构，也不定义正式运行时协议、状态机或领域数据模型。

## 当前结构

| 路径                      | 当前职责                                            | 阶段 0 明确不包含                           |
| ------------------------- | --------------------------------------------------- | ------------------------------------------- |
| `apps/desktop`            | Tauri 2、React、TypeScript 和 Ant Design 的桌面空壳 | 账号、项目、终端或额度 UI                   |
| `apps/desktop/src-tauri`  | 最小 Tauri Rust 入口与最小能力配置                  | 自定义 IPC、shell、文件系统、网络或进程权限 |
| `crates/muxlane-core`     | 共享核心 crate 边界与非业务构建元数据               | Account、Project、Runtime 领域模型          |
| `crates/muxlane-protocol` | 未来组件协议 crate 边界                             | JSON-RPC 方法、wire type 或序列化契约       |
| `crates/muxlaned`         | 将来 WSL daemon 的二进制名称和元数据输出            | daemon 启动、服务、tmux 或 WSL 控制         |
| `crates/muxlane-cli`      | 将来 WSL CLI 的二进制名称和元数据输出               | project、account、recover 等命令            |

## 后续设计入口

以下内容必须在对应阶段通过 ADR 和可验证实现正式设计：

- Account、Project、Runtime 的领域边界和持久化模型。
- `CODEX_HOME` 的项目隔离策略与凭证生命周期。
- daemon 与 GUI/CLI 的协议、认证边界和错误模型。
- 启动事务、锁、崩溃恢复与冲突处理。
- tmux、终端桥接、Codex CLI/App Server 的运行边界。
- 配置资产、Skills、MCP 与 Plugins 的治理模型。

阶段 0 不使用空 trait、虚构数据或占位 RPC 来预先冻结这些决策。

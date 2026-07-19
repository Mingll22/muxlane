# ADR-0013: Reframe Phase 7 as a Safe Local Workbench

- Date: 2026-07-20
- Status: Accepted

## Context

阶段 1 把 Phase 7 描述为统一 Asset 治理和 CodeMirror 文件工作台。Phase 6/7 实施前重新评估发现，Skills、MCP、Plugins、可执行仓库配置和内嵌写文件会同时扩大供应链、任意执行、路径授权和凭证泄漏边界；它们不应为了工作台可用性被捆绑进入同一个阶段。

## Decision

Phase 7 当前范围改为安全的本地开发工作台：终端优先布局、Project/Terminal 快速切换、非秘密 Project template、用户显式触发的 command preset、按 Project/Terminal/Thread 隔离的输入历史、专注/全屏模式，以及严格受 canonical Project root 限制的只读文件 list/search/preview/location。

明确延期 Skills、MCP、Plugins、统一 Asset 治理、CodeMirror、内嵌编辑，以及文件新增、保存、重命名和删除。延期能力不属于 Phase 7 当前完成条件，也不得在 Phase 8 发布工作中顺带实现。

## Consequences

- GUI 能覆盖高频开发循环，同时保持 daemon 为 Account、Project、Runtime、Launch、Recovery、Usage 和 Terminal 的事实来源。
- Template、preset 和 input history 只保存非秘密配置或明确提交的输入；不保存 Terminal 输出。
- 文件能力保持只读，拒绝路径穿越、符号链接、二进制和超限文件。
- 未来重新引入延期能力时需要新的需求、安全模型、协议能力和 ADR。

## Alternatives

- 保留阶段 1 的完整 Asset/CodeMirror 范围：拒绝，因为会把多个高风险边界压入同一阶段。
- 用其他编辑器替代 CodeMirror：拒绝，因为这只替换实现而不减少写文件授权风险。
- 完全取消文件导航：拒绝，因为只读定位和预览对终端工作台有直接价值，且可在严格边界内实现。

## Security impact

WebView 不获得任意 Shell、filesystem、WSL 或 tmux target。所有 workspace 解析由 `muxlaned` 在 canonical root 下完成；外部打开只接受 daemon 返回的 Windows canonical path。历史拒绝常见 secret marker 和超限输入，诊断默认排除输入内容。

## Compatibility impact

Protocol 1.0 增加 `workbench.*` 和 `workspace.*` typed capability；旧客户端通过 capability negotiation 忽略未知能力。SQLite 以前向 migration 升级到 schema v5，旧写入端不得打开未来 schema。

## Supersedes

阶段 1 文档中把 Phase 7 固定为 Asset + CodeMirror 的范围描述；不替代 ADR-0001～0012 的 Runtime、协议或数据不变量。

## Superseded by

None.

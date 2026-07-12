# ADR-0010：Versioned Capability Negotiation

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

GUI、Daemon、CLI 和可选 CodexAdapter 的发布节奏不同。仅根据版本字符串猜测某个方法、Schema 字段或安全语义是否存在，会在部分升级、未知字段或上游实验性接口变化时产生错误写操作。兼容策略需要同时表达协议范围、实际功能和安全降级。

## Decision

所有本机 Muxlane Client 在每次连接/重连时进行版本握手，声明最小/最大 Protocol Major.Minor 和显式 capability names。功能可用性由协商 capability 决定，而非只由版本字符串推测。无安全协议交集或缺少关键能力时禁止业务写操作；在不泄露秘密的前提下可保留最小只读诊断。

Codex CLI/App Server 的能力经官方 Schema 或无副作用 probe 归一化到内部 CodexAdapter。上游候选字段、方法和版本缓存不是 Muxlane 稳定公共合同。

## Consequences

- GUI、Daemon 与 CLI 必须实现握手、能力清单、明确 degraded/incompatible 结果和升级提示。
- 新 Minor 只能增加可选字段/能力；Major 变化需明确兼容策略。未知能力不能调用，未知 enum 安全降级。
- 版本仍保留用于诊断、已探测能力缓存和支持窗口，但不能绕过 capability probe。

## Alternatives

- **只比较版本字符串：** 无法表达 backport、feature flag、上游实验性字段或同版本不同平台能力。
- **固定单一全栈版本：** 降低短期复杂度，但阻断诊断、渐进升级和发布恢复。
- **假定所有已安装 Codex CLI 能力相同：** 将未验证上游 Schema 误当稳定合同。

## Security impact

不兼容或能力未知时保守拒绝写操作，避免旧 Client 发出新语义的危险请求、或 GUI 误解释上游数据。能力清单、错误和兼容警告不得携带 Token、原始上游响应或敏感路径。

## Compatibility impact

定义 Protocol Major/Minor 与 capability window；具体 Windows—WSL Bridge、wire framing 和 Codex App Server Schema 仍待 POC。未来若改变协商语义，必须提供 Major 迁移策略或以新 ADR 替代。

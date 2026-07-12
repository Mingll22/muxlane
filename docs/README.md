# 文档索引

阶段 1 的下列文档均为 **Frozen**：这是基于当前证据的设计冻结，不代表业务能力已经实现。后续 POC 若推翻某项已接受决策，必须通过新的 ADR 修订；不得静默改写历史 ADR。

| 文档                                        | 状态     | 权威范围                                | 相关阶段  | 相关 ADR              |
| ------------------------------------------- | -------- | --------------------------------------- | --------- | --------------------- |
| [产品需求文档](PRD.md)                      | Frozen   | 产品范围、需求和需求追踪                | 1；2–8    | 0001–0012             |
| [总体架构](ARCHITECTURE.md)                 | Frozen   | 系统边界和高层运行模型                  | 1；2–8    | 0001–0012             |
| [威胁模型](THREAT_MODEL.md)                 | Frozen   | 资产、边界、威胁和安全验收              | 1；2–8    | 0001–0012             |
| [Runtime 生命周期](RUNTIME_LIFECYCLE.md)    | Frozen   | Launch、锁、凭证与关闭生命周期          | 1；2–4    | 0002、0003、0005–0008 |
| [持久恢复状态机](RECOVERY_STATE_MACHINE.md) | Frozen   | Transaction 状态、Hash 决策和 Recovery  | 1；4      | 0003、0005–0009、0012 |
| [逻辑控制协议](PROTOCOL.md)                 | Frozen   | Protocol v1 Candidate、能力与数据面边界 | 1；3、5–6 | 0004、0010、0011      |
| [逻辑数据模型](DATA_MODEL.md)               | Frozen   | 逻辑实体、事实来源和迁移边界            | 1；4–7    | 0006、0009、0012      |
| [兼容策略](COMPATIBILITY.md)                | Frozen   | Supported Target、探测和验证矩阵        | 1；2–8    | 0004、0010–0012       |
| [架构决策记录](adr/README.md)               | Accepted | 长期设计取舍及替代关系                  | 1；2–8    | 0001–0012             |

推荐阅读顺序：PRD → 总体架构 → 威胁模型 → Runtime 生命周期 → 恢复状态机 → 协议 → 数据模型 → 兼容策略 → ADR。

补充入口：[根目录架构摘要](../ARCHITECTURE.md)；[研究材料约束](research/README.md)。

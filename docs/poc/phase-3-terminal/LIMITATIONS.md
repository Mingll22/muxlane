# 限制与下一步

## 核心阻断

1. 当前环境无法运行或检查真实 Windows Tauri Host；因此 GUI close/kill/reopen 和 Host→WSL 真实链路未验证。
2. history snapshot 与 Control Mode attach 没有原子顺序边界；无损、无重复恢复未证明。
3. 没有实际运行多 Project、多 Window、Ctrl+C、连续大输出和慢消费者 E2E。

## 非核心工程限制

- Gateway 目前依赖 stdout pipe 阻塞形成背压，未提供 production-grade per-client queue、丢弃计数或公平调度。
- Windows Host 假定 WSL 内的 `muxlaned` 已可执行；POC 没有安装、打包或发现机制。
- 前端 production bundle 有 Vite >500 kB 提示；本阶段没有为此引入无关的拆包重构。

## 后续阶段边界

这些限制不能通过提前实现 Phase 4 Crash Recovery、Phase 5 生产 Daemon/数据库、Phase 6 产品管理或 Phase 7 工作台解决。先在具备 Windows Tauri toolchain 的环境修复阶段 3 核心门禁，再决定是否进入下一阶段。

# 限制与下一步

## 核心门禁

Phase 3 的 history/live、连接隔离、Windows 原生 GUI、Ctrl+C、resize、GUI close/kill/reopen、Session 恢复与多 Project/Window 核心门禁均已通过；当前没有 Phase 3 核心阻断。

## 非核心工程限制

- Gateway 是 stdio POC，不是生产 daemon；有明确有界队列和溢出失败，但没有多租户公平调度、长期 metrics 或 production supervision。
- Windows Host 依赖 WSL PATH 中已有受控 `muxlaned`；POC 没有安装、签名、打包、升级或可执行文件发现机制。
- history 仅保留 300 行；这是有限 attach/recover bootstrap，不是持久历史存储。
- Windows/WSL 重启恢复、Crash Recovery、Project/Account Lock 和持久事务属于 Phase 4，未实现、未验证。
- Vite production bundle 有单 chunk 大于 500 kB 提示；本阶段未做无关拆包。
- `cargo audit` 保留 17 个 allowed transitive warning，主要来自既有 Tauri Linux GTK3 依赖图；未发现可阻断 vulnerability。
- WSL 未安装 Desktop 的 GTK/GLib 开发包；Desktop 完整 check/clippy/test/build 在 Windows MSVC 完成。

## 下一阶段边界

允许在 Phase 3 PR/CI、review 与 merge 全部收口后进入 Phase 4。Phase 4 必须另立任务，不得把本 POC 升格为正式生产终端，也不得提前实现 Phase 5～7 能力。

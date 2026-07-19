# Phase 4 Crash Recovery POC

本目录提供只使用 synthetic credential fixture 和隔离 `/tmp/muxlane-phase45.*` 数据根的可重复故障注入。它不会读取全局 `~/.codex/auth.json`，也不会调用真实账号 API。

运行前先构建正式 daemon/CLI：

```bash
cargo build -p muxlaned -p muxlane-cli
poc/phase-4-recovery/fault-injection.sh
```

脚本硬编码拒绝非 `/tmp/muxlane-phase45.*` 根，并验证：

- daemon 单实例、Unix Socket `0600`、无 muxlaned TCP listener；
- daemon 强杀后 Runner/Codex 继续，真实 `flock` 阻止错误 Recovery；
- Codex `SIGKILL`、Runner `SIGKILL` 与 Ctrl+C 后的签回/Recovery；
- 同 Account 与同 Project 竞争拒绝，不同 Account+Project 并行；
- Runtime 活动凭证最终清理；
- 当前 Codex App Server schema probe；无效 synthetic 凭证查询失败后 Query Home 清理；
- SQLite、日志和诊断导出不含 synthetic credential 内容或 Authorization/Bearer 材料。

Hash 冲突矩阵、损坏 JSON、checkout/commit 中断、终态不可变、重复 Recovery、stale PID/PID reuse/boot mismatch 由 `crates/muxlane-core/tests/recovery_matrix.rs` 与单元测试覆盖。

当前 POC 不能宣告完成：真实 `wsl --terminate` 会终止承载当前 Codex Session 的发行版，尚未在独立测试发行版执行；正式 Terminal data plane 也尚未从 Phase 3 compatibility surface 迁移。

# Phase 4 Recovery Results

## 2026-07-19 WSL/Linux isolated run

命令：

```bash
cargo build -p muxlaned -p muxlane-cli
poc/phase-4-recovery/fault-injection.sh
```

最近一次真实输出：

```json
{"scenario":"daemon_kill_then_codex_kill","status":"PASS"}
{"scenario":"runner_kill","status":"PASS"}
{"scenario":"ctrl_c","status":"PASS"}
{"scenario":"lock_contention_and_parallelism","status":"PASS"}
{"scenario":"usage_probe_failure_cleanup_and_diagnostics_redaction","status":"PASS"}
{"status":"PASS","evidence_root":"/tmp/muxlane-phase45.tiw5Ao"}
```

该 evidence root 是本机临时时点证据，不是 Git 产物。检查确认各场景最终无 Project Runtime `auth.json`，根/DB/Socket 目录模式分别为 `0700`/`0600`/`0700`，daemon-kill 与 Ctrl+C 为 `finished`，Runner kill 为 `recovered`，并行场景两条事务均独立结束。

Rust 恢复矩阵另覆盖 checkout 边界、Runtime-only 刷新、较新 Vault 保留、双方变化冲突、损坏 JSON、重复 Recovery，以及 Vault 原子替换成功但事务状态尚未推进时的恢复。

## 未通过或未运行

| 门禁                                      | 状态      | 原因                                                                                         |
| ----------------------------------------- | --------- | -------------------------------------------------------------------------------------------- |
| 真实 `wsl --terminate` / boot change      | `NOT RUN` | 当前发行版承载本 Codex Session；不能安全自终止并继续收证。                                   |
| Recovery 中再次 kill daemon               | `NOT RUN` | 尚未加入独立发行版外部 orchestrator。                                                        |
| 正式 Terminal live data plane             | `BLOCKED` | create/list/history 已正式化；attach/switch/close 与 Control Mode live stream 仍未正式实现。 |
| 真实用户 Account Usage success smoke      | `NOT RUN` | 未获授权读取或复制全局真实凭证；仅 schema probe 与 fixture failure path。                    |
| Windows Desktop / Windows-WSL integration | `NOT RUN` | WSL `verify:desktop` 因缺 `pkg-config`/GTK 系统库而 `BLOCKED`；Windows 原生未运行。          |

因此 Phase 4 总体结论仍是 `BLOCKED`，Phase 5 不允许宣告完整或合并。

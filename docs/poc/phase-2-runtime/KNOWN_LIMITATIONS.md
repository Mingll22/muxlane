# 阶段 2 Runtime POC 已知限制

1. 本次真实请求没有自然产生 Credential Mutation；Token Refresh 结论为 `NOT OBSERVED`。
2. 当前环境没有 `strace`，因此未以 syscall 级证据证明 Codex 从未访问全局 Codex Home；文件系统隔离证据仍为 PASS。
3. 当前开发 Session 使用 API-key-only 认证上下文，不能与 OAuth 候选账号做同类型身份指纹比较。
4. 一个未选候选账号的首次最小请求因额度失败；没有重试，也没有查询精确 reset 时间。两个不同 OAuth 账号仍完成了 2C。
5. Credential Harness 是单进程、顺序执行的非生产 POC。它没有 Account/Project `flock`、SQLite transaction、Runner identity、崩溃恢复或自动冲突处理。
6. 本轮只验证正常退出、CLI 非零退出、Session 恢复失败和文件系统负向场景；没有杀死 Daemon/Runner、重启 WSL、模拟断电或实现阶段 4 Recovery Manager。
7. WSL 本地 `verify:desktop` 仍可能因缺少 GTK/GLib/WebKit/pkg-config 系统依赖而阻塞；Windows CI 是 Desktop Rust 的主要证据。
8. POC 只针对本地 Codex CLI `0.144.1` 和本次已观察能力；没有冻结上游内部 Schema 或保证其它版本行为。

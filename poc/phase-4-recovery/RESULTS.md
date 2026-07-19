# Phase 4/5 Recovery and Runtime Results

## 2026-07-20 conclusion

Phase 4 conclusion: `PASS`.

Phase 5 conclusion: `PASS`.

The stage was squash-merged by PR #12 as `b904ef7b156fca1d059062db14a4b27513d93c9e`. PR CI run `29703570077` and post-merge `main` CI run `29703706489` passed all Rust, frontend, Windows Desktop, and dependency-security jobs. The phase branch and isolated test environments were removed. No real Account credential was read or queried.

## Real destructive recovery evidence

All credentials were synthetic fixtures. The default Ubuntu distribution and `docker-desktop` were not terminated.

### Dedicated WSL distribution

Distribution: `Muxlane-E2E-CODX-R8M4QZ`, installed under a dedicated `E:` test location.

Observed results:

```text
scenario=checkout_boundary_kill state_before=preparing recovery=checkout_boundary_cleaned state_after=recovered runtime_auth=absent open_incidents=0
scenario=commit_after_atomic_vault_kill state_before=committing_auth hashes_equal_before=yes recovery=runtime_credential_committed state_after=recovered vault_stable=yes runtime_auth=absent
scenario=before_terminate boot_id=84d7ecf4-f44f-47c6-b5db-ae890c4a1b17 state=running
scenario=recovery_journaled running_recovery_runs=1
scenario=recovery_process_killed
scenario=post_restart_idempotent_recovery state=recovered repeat_results=0 incomplete_runs=1 completed_runs=1 runtime_auth=absent vault_stable=yes open_incidents=0 new_launch_state=finished socket_mode=600 root_mode=700
```

`wsl.exe --terminate Muxlane-E2E-CODX-R8M4QZ` was executed while the managed transaction was `running`. The distribution was observed as `Stopped`, then restarted. Recovery was killed after its durable `recovery_runs.status=running` journal entry and restarted a second time. The unfinished audit row remained evidence, a new run completed, repeated Recovery returned zero work, locks were reusable, and a new Launch finished.

WSL2 distributions share the utility VM kernel, so terminating one distribution does not change `/proc/sys/kernel/random/boot_id` while another distribution remains running. This is an observed platform fact, not a simulated pass.

### Isolated real Linux boot identities

An Ubuntu 24.04 systemd-nspawn root was created inside the dedicated test distribution. Two separate container boots exposed distinct Linux boot identities to the real Muxlane processes:

```text
before: 2043cb04-72fb-4cbc-8238-e6b426cc7721
after:  549b81b0-df36-4df7-838d-715d096d64fe
```

The first boot left a real `running` Launch and active Runtime credential. The second boot recovered it to `recovered`, preserved the Vault hash, removed Runtime `auth.json`, produced no incident, returned zero work on repeated Recovery, and completed a new Launch. This validates the `boot_id + PID + start_ticks + executable identity` classification against a real boot-identity transition without shutting down the user's default WSL or Docker.

## Formal Terminal and Windows/WSL evidence

The formal `terminal-gateway` uses the production `muxlane-runtime` tmux socket and a protocol separate from the retained Phase 3 POC frames.

Automated Linux integration verified handshake, attach, one-shot history, live output, input, resize, bounded queues, parallel Projects, detach, reconnect, stale stream rejection, and close. Windows PowerShell then acted as the host client against the dedicated WSL distribution and verified control handshake, attach/start/input/resize/detach, reconnect history, switch, close, daemon/CLI state consistency, and no Windows TCP listener owned by the WSL client process:

```json
{
  "scenario": "windows_wsl_formal_control_and_terminal",
  "status": "PASS",
  "protocol_major": 1,
  "reconnect_history": "PASS",
  "tcp_listener": "absent"
}
```

Windows MSVC/Tauri evidence at the same commit:

- Desktop `cargo check`: `PASS`;
- Desktop Clippy with `-D warnings`: `PASS`;
- Desktop Rust tests: 3 passed;
- Desktop release build: `PASS`;
- native `Muxlane.exe` run: process started, obtained a real main-window handle, and exited cleanly with code 0;
- Windows frontend typecheck/test/build: 5 tests passed and production build completed.

## Automated recovery and domain matrix

- daemon/Runner/Codex/control-client exits, Ctrl+C, double-lock contention, parallel Projects, stale PID/PID reuse, damaged Runtime JSON, newer Vault, Runtime-only refresh, simultaneous Vault/Runtime changes, post-Vault atomic interruption, repeated Recovery, and terminal transaction immutability: `PASS`;
- explicit Incident `keep_vault` resolution: idempotent and audited; evidence retained, Launch unblocked without rewriting the terminal transaction;
- Session/Thread index: Project-local metadata only; prompt/session content is not copied into SQLite;
- Project archive: refuses active/recovery state, preserves Runtime/files, and blocks subsequent Launch;
- Usage: fake App Server handshake and semantic mapping, 300-minute and 10080-minute windows, Reset Credit, token usage, isolated Query Home, and global four-query concurrency: `PASS`;
- real Account Usage success smoke: `NOT RUN` because the user did not authorize access to a real credential.

## Security and quality

- `pnpm verify`: `PASS`;
- `pnpm audit --audit-level moderate`: no known vulnerabilities;
- `cargo audit`: no blocking vulnerability; 17 allowed unmaintained/unsound warnings remain in the existing Tauri/GTK transitive allowlist;
- diagnostic and repository scans found no real credential, Token, Cookie, Authorization value, private key, prompt body, or Terminal content;
- controlled root/Socket/Vault/Runtime/recovery evidence modes were checked as `0700`/`0600` as applicable;
- Linux Desktop build in the default WSL is `BLOCKED` by missing `pkg-config`/GTK development packages; Windows native Desktop validation passed and is the applicable target gate.
- PR #12 had no review threads. The automated Codex review did not run because its service quota was exhausted; this is recorded as `NOT RUN`, not as a review pass.

## Defects found by real fault injection

Real WSL restart exposed tmux window identity reuse (`@0`) across tmux server lifetimes. SQLite schema v4 now keeps historical Terminal rows while applying uniqueness only to active Terminal records. A second defect allowed a very fast Codex exit to race process identity capture; the Runner now treats an already-observed child exit as a normal lifecycle event and commits/cleans credentials safely.

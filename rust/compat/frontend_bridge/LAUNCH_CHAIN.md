# Local native launch chain: verified with dedicated guard

**Current status: passed with explicit `--launch-only-guard`.** Independent
static review preceded candidate execution. Its self-tests and both asserted
benign spawn gates passed before the actual native Session/Core/native-child
chain passed: 1 passed, 0 failed, 0 ignored. See `LAUNCH_GUARD_VALIDATION.md` and
`traces/native-launch-frontend-shape-20261106/provenance.json` for the final
capture, whose actual request fields match the frontend's generated shape.

The original default guard still cannot support this std pre-exec path. Its
blocked result remains preserved in commit `21c2e41` and
`launch_chain_blocked_evidence.json`. The separate candidate changes only the
owned test isolation mechanism, not production Core, system permissions or the
original guard. This is still not Java/game/UI acceptance.

## What is prepared

`run_launch_chain.py` is an independent, default-off runner. It copies the same
unchanged frontend Session/transport modules as the existing instance runner,
builds one exact `launch_chain` test offline/locked, and separately compiles a
benign Rust child with no dependencies. Existing runners, CI pins, production
code and `network_guard.py` are unchanged. The optional base64 dependency is
enabled only by `local-native-launch-chain`. Ordinary default builds do not
include this test. `--all-features` would enable it, so is not a safe substitute
for the exact runner.

The runner verifies all three imported modules are byte-identical between
frontend `c0107f1803c73398a99f8ddd27ed712c0ed2e1b4` and current-main snapshot
`90b9d5472e620255af740b55315e2cf75c1c9115`, using already-local Git objects.
No fetch occurs. It freezes and hashes Core and the compiled native fixture.

```
python rust/compat/frontend_bridge/run_launch_chain.py \
  --frontend /path/to/clean/c0107f/frontend \
  --current-frontend-repo /path/to/repo/containing/90b9d547 \
  --rust /path/to/jmcl-core-rust --launch-only-guard \
  --evidence-dir /path/to/new/owned/evidence-directory
```

`--build-only` runs no Core, child, guard probes or tests and cannot be combined
with `--evidence-dir`. The original guard remains the default; the new guard
requires `--launch-only-guard`. That option requires all candidate self-tests
and both missing/successful benign spawn gates before Core can start, followed
by the exact single-test inventory and one passed/unignored test.
`--evidence-dir` records an original Session callback/Result trace only after a
successful run. No trace is invented for the earlier failed run.

The verified dedicated-guard chain covers:

- synthetic cached manifest/version data and five ordinary `hello` bytes at the
  expected client-artifact path, without archive parsing or installation;
- explicit absolute owned-native-child override and local Java policy, empty
  default-empty candidate list (the frontend omits `java_paths`), no runtime
  fallback/acquisition, and no real account;
- missing override error after preparation, cleanup, and same-session retry;
- a real benign native child checking literal argv, working directory, prepared
  native directory, inherited seccomp and filtered environment;
- started event, redacted argv/secrets across child writes, base64 stdout/stderr,
  exact empty newline preservation, a 36-KiB line framed at 16-KiB including a
  split UTF-8 code point, and binary stderr at EOF;
- exit 7 versus 0 as terminal RPC results, reaped process, native cleanup,
  following ping/relaunch, close twice, closed-session rejection and a new Core
  on reopen.

This would still not be Tauri IPC/UI, a JVM/game, real authentication, loader or
archive compatibility, Java acquisition, running-process cancellation or
cross-platform acceptance.

## Historical original-guard blocker

Recorded toolchain: Rust 1.99.1, commit
`58cf8f9c35b355199727627d07ae34a4e2846d67`, Linux x86_64 GNU.

1. Guard self-tests pass, including all 16 denied syscalls, architecture/x32,
   descriptor handling, exec inheritance and allowed local IPC.
2. The actual chain reaches a real native Session handshake. Its first
   `launch.execute` request, with an explicitly missing owned-child override,
   returns `SessionError::Transport { code: "CORE_RPC_EOF", ... }` rather than
   the expected `JAVA_NOT_FOUND`. Test result: 0 passed, 1 failed.
3. A minimal separate Rust program calling standard `Command::spawn` with an
   empty `pre_exec` closure is killed by the unchanged guard for both a missing
   path and a successful spawn of its own benign child. The successful child
   prints `benign child started`; its parent exits by SIGSYS (-31). Removing the
   failure case therefore cannot make this native launch chain run.
4. Static disassembly of the tested Core shows standard `Command::spawn` calls
   libc `recv` in its fork/pre-exec parent-side error channel (address
   `0x1000fc5` in that build). Linux libc `recv` uses `recvfrom`, one of the
   unchanged guard's prohibited syscall numbers. Core's
   `java/direct_exec.rs::configure` deliberately selects `pre_exec` for
   Zig-compatible parent-PATH/direct-exec semantics and no shell fallback.

`tests/fixtures/spawn_guard_probe.rs` retains the minimal reproducer with an
explicit guard-mode assertion. It is not an automatic test target. The recorded
diagnostic runs used the equivalent small local source before this documented
version was added. This retained version was compiled, not rerun.

`strace` was unavailable because the environment rejected PTRACE_TRACEME with
`Operation not permitted`. It was not retried or worked around. No syscall
trace is claimed; the diagnosis combines guarded minimal-program results and
static disassembly. The attempted unstable `create_pidfd(false)` configuration
was rejected by the stable compiler and was never executed; it would not solve
the unconditional error-channel recv path anyway.

The original blocked pass stopped without changing the guard or production
subprocess code. Later authorization allowed a separately reviewed owned
launch-only guard, whose narrowly constrained local receive exception and
mandatory import/injection denials are documented in `LAUNCH_GUARD_DESIGN.md`.
No environment restriction was bypassed, and no unguarded rerun, runtime
download, Java execution or default-CI change was used. The old result remains
historically true; the new passing result names its different executing guard.

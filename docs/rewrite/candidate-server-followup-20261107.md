# Linux server follow-up, 2026-11-07

This is new execution after the [first Linux follow-up](candidate-followup-20261107.md).
Unlike that earlier no-real-Java slice, this slice **executes the existing
OpenJDK 21 for version/property probes**. All four server child executions are
owned synthetic shell fixtures. No real Minecraft server is run.

## Exact subject

- Executed local commit: `031098063d01bbdc0a72e8aecff10982dd2eefc7`.
- Executed tree: `faa9d6ad38435135287fad02bfdf8b28704afb7d`.
- Previous preserved remote head: `2727ce6428042c1b1bd2952419c6b13f576f7f8b`,
  tree `5e61790fde937ab927b2b79d1a59d3323f2dbaa3`.
- Changes before execution: the separately reviewed metadata-status documents,
  a test-only major-99 isolation mode and fixture assertions in
  `rust/tests/server_lifecycle.rs`, and its [isolation contract](../../rust/compat/server-lifecycle-fixture-20261107.md).
- Rust/Zig production sources, dependency manifests/locks and workflows are
  unchanged. Core executable SHA-256 remains
  `dad858d76f05cc4fad65814b03790a83e02e9244c5fa1d3155f882b89c1fd1f5`.
- Built test executable SHA-256:
  `e9d8b92e5f576d831ddccad2a6698d7914e0180eda99106ef527e94429053b93`.

The targeted test source was rebuilt with `--offline --locked --no-run` and
main-crate `--all-targets` Clippy `-D warnings` passed. Both checks occurred on
exactly the test source subsequently committed and executed; neither was a full
runtime test-suite execution. Existing Rust 1.99.1 and cached dependencies were
used on Linux x86_64 GNU.

## Results and boundaries

Each built test was selected by exact name with
`--exact --nocapture --test-threads=1`, in this order:

1. `detached_start_command_status_restart_stop`: **1 passed, 0 failed, 0 ignored**,
   7 filtered out. Actual Core RPC start, detached status, repeated-start error,
   console command, stdout/stderr logs, running-properties guard, restart and
   graceful stop. Start and restart each selected the owned fixture.
2. `stop_escalates_only_verified_synthetic_children`: **1 passed, 0 failed,
   0 ignored**, 7 filtered out. Two owned fixtures exercise verified TERM and
   KILL escalation, including readiness after the TERM-ignore trap.

An independently reviewed external runner checked three system Java aliases
against their common expected OpenJDK-21 executable and release metadata. The
minimal process environment, all candidate paths and hashes are retained in
[the manifest](../../rust/compat/evidence/server-lifecycle-20261107/manifest.json).
Preflight really probed each alias, and the production RPC performs discovery
and probing again before its strict major/architecture selection. The fixture
reports major 99 and the receipt requires 99. The test verifies that the
installed major-21 runtimes alone produce `NoCompatibleJava` for that request.
The runtime-failure RPC branch itself is not separately executed here.

Every successful start/restart checks the canonical fixture path in the RPC
response and state, the returned PID, the production random-identity match and
the fixture path in `/proc/PID/cmdline`. The supervisor executes this owned
shell file with `-jar server.jar nogui`; the six-byte `server.jar` is plain
`benign`, not a real archive. No system JVM is launched as the server.

After each test, the external runner found no live command line belonging to
any observed owned child or supervisor, and the dedicated temporary directory
was empty. No automatic timeout, manual signal or residual cleanup was needed.
This is not a claim about immediate OS reaping of exited processes. The tests
retain owned state on cleanup failure, and the runner never kills by name.

## Separate metadata status

The [diagnostic audit](../../rust/compat/metadata-diagnostic-audit-20261107.md)
was independently executed against the prior `5e61790f` tree: 25 existing tests,
5,015 source-equal comparisons and one separately asserted accepted URL case.
Its four documentation-only changes were reviewed and included before this
server run. No production/parser/fixture change or new red-to-green repair is
claimed. The old blanket claim of unfinished concrete Version/CurseForge
malformed-JSON cases was stale; exhaustive and live-provider equivalence remains
unproven. COMPAT-URL-001 is unchanged and is not a raw-URL implementation blocker.

## Still outside this result

No Java runtime download, automatic acquisition, installation, runtime archive,
real Minecraft game/server, real authentication/provider, RCON/Query sockets,
Tauri UI or Windows/macOS native execution. No full test suite or broad remote CI
was enabled. Installed-JVM probing was authorized ordinary discovery, not a
permission bypass; no tool denial occurred.

Server world backup/restore and managed RCON/Query remain separate tests. Any
next server-start test needs the same reviewed major-99 selection and cleanup
controls, including bounded local-socket behavior where relevant. This evidence
does not promote Rust or change `replacement_allowed=false`.

## Retained evidence

[`rust/compat/evidence/server-lifecycle-20261107`](../../rust/compat/evidence/server-lifecycle-20261107)
contains the exact external runner, compile/Clippy logs, lifecycle log and
SHA-256 manifest. Log files preserve process output; the lifecycle file also
contains runner-generated JSON command, identity and cleanup records.

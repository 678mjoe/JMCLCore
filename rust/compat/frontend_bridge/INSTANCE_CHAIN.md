# Local isolated ordinary-instance native bridge chain

This separate opt-in lane uses unchanged frontend modules from
`c0107f1803c73398a99f8ddd27ed712c0ed2e1b4`, not the legacy bridge runner's older
pin. It does not change existing bridge CI, frontend shipping pins, or Core
production code. It supports Linux x86_64 LP64 only and refuses other hosts.

## Preparation versus execution

Build dependencies may be fetched from the official Cargo registry using the
existing committed lockfile (`cargo fetch --locked`). This is preparation, not
an offline-runtime claim. All subsequent builds use `--offline --locked`.
The runner stages byte-identical `session.rs`, `transport.rs` and
`modpack_preview.rs`, verifies the frontend revision and tracked cleanliness,
prints module hashes, freezes a real Core binary and prints its SHA-256.

Run after building the ordinary Core binary offline:

```
python rust/compat/frontend_bridge/run_instance_chain.py \
  --frontend /path/to/clean/frontend \
  --rust /path/to/jmcl-core-rust
```

`--build-only` compiles the one selected test without running isolation probes
or Core. The standalone harness feature `local-native-instance-chain` is
default-off. Existing workflow commands do not enable it. `--all-features`
would enable this feature, so that is not equivalent to keeping this lane out.
The supported execution path is this runner, not direct `cargo test`.

## Verified isolation boundary

Before any Core starts, `network_guard.py` actively probes all blocked syscall
numbers and requires SIGSYS, verifies the architecture/x32 guards, verifies
socket/connect remain blocked after exec, checks permitted AF_UNIX socketpair
read/write, and checks rejection/removal of inherited socket descriptors.
If setup or any probe fails, execution stops, with no fallback route.

The runner then execs the selected test executable under a process-local,
inherited seccomp filter with `no_new_privs`. All of its threads and Core
children inherit the filter. It closes extra inherited descriptors and rejects
sockets on stdio. It denies socket creation, connect, network send/receive,
bind/listen/accept and io_uring operations. AF_UNIX socketpair is allowed only
for local IPC used by Tokio. Unknown audit architectures and x32 syscall
numbers are killed. Core dumps are disabled for these children.

This narrows the capabilities of trusted test binaries and their children;
it does not modify persistent or system network/security settings. It is not
a general sandbox for hostile code: for example, it does not claim to defend
against malicious attempts to obtain another process's descriptors through
ptrace/pidfd mechanisms. No networking request attempt is an acceptable test
result, even if zero bytes are eventually transferred.

## Evidence and limits

The real native Session launches real `Core rpc` children in an owned temporary
cwd, so top-level protocol dispatch and the production HTTP-transport/cache
selection path run unchanged. Fresh manifest/version caches and a CAS object
containing only `hello` satisfy every planned input. There are no loaders,
assets, native archives, Java probes, real accounts, or existing user instances.
The `.jar` destination is a production path; its bytes are not an archive and
are never parsed or executed.

Assertions cover three instance create/get operations, cached execute and
prepare, progress/file callbacks and zero transferred bytes, shared read-only
CAS inode identity, independent receipts, a prepare error without rollback of
the download receipt, close twice, rejection after close, spawning a new Core
on reopen, list/get installed state, repeated cached execution and independent
receipt removal. The runner checks the exact single test inventory and requires
one passed/unignored test, preventing a zero-test false green.

This is stronger than the earlier injected handler-chain test. It is still not
Tauri IPC, packaged UI, frontend task-state acceptance, real provider/TLS,
archive preparation, Java acquisition, game launch, non-Linux evidence, or
replacement readiness. A matching download receipt can make installed=true
even when preparation failed; this existing contract is preserved.

Recorded local verification is in `instance_chain_evidence.json`. The isolated
native test and Python/Rust formatting checks passed. Default Clippy completed
with five pre-existing warnings in the unchanged frontend `transport.rs`;
strict `-D warnings` failed on four async-trait `double_must_use` warnings and
one `while_let_loop` warning. Those imported files were not modified or silenced.

# Bounded current-frontend bridge gate

This test-only harness imports **unchanged files** from frontend
`91ec19d8125942296054d076013a7ae538f64c5d`. It does not change the frontend
shipping pin, package an app, or establish replacement readiness.

```sh
# Prepare exact frontend checkout + bun install --frozen-lockfile there.
# Build Rust and pinned Zig 5a4310debbcee26b3d2391bab2339f79cd6ec52e separately.
python rust/compat/frontend_bridge/run.py --frontend /path/to/frontend \
  --rust /path/to/jmcl-core-rust --zig /path/to/jmcl-core
```

Requires Rust 1.99.1, Bun 1.4.4, Python 3, Git and the frontend's installed
locked JS dependencies. The committed standalone Cargo.lock is required.
The runner refuses another frontend revision or tracked source modifications,
freezes both binaries in a temporary directory and prints their SHA-256s.
`CARGO_TARGET_DIR` may share compiled dependencies; staged native tests are
fresh copies so `env!("CARGO_MANIFEST_DIR")` cannot point at a deleted prior run.

## Three separate evidence tiers

1. **Mock transports:** unchanged frontend `session.rs` unit tests exercise
   close during hanging writes/requests, queued requests, bounded abort,
   credentialed single-use sessions and protocol classification. These are
   deliberately not called real-sidecar or credential acceptance.
2. **Typed wrappers / real Core:** unchanged `test-account-core.ts` and
   `test-capabilities-core.ts` run on each binary. They use
   `CoreSession.createForTesting` to inject a direct process request adapter;
   they do not traverse the native bridge or actual Tauri IPC. The account
   wrappers now use the corrected envelope, unlike the historical 19/11 smoke.
   Capability cases stop at parameter guards: no Java execution/acquisition.
3. **Unchanged native bridge / real Core:** import byte-identical `session.rs`,
   `transport.rs`, `modpack_preview.rs` and `tests/rpc_smoke.rs`, without Tauri
   dependencies. On each binary run the inherited synthetic account, world,
   receipt identity, zero-download modpack, protocol, error recovery, queue
   and empty-runtime-list cases. The sole inherited system-Java detection
   test is explicitly filtered out; Windows additionally runs the existing
   no-console/piped-echo fixture. The new cached-modpack test requires real
   file/progress callbacks, cached status, zero transferred bytes and no retry;
   then idle close twice, rejection after close and reopen with preserved disk
   bytes. Import is explicitly **not** game installation.

No actual Tauri IPC/channel compaction, native credential injection/keychain,
sidecar discovery, packaged application/UI, real authentication, provider,
TLS/redirect acceptance, game/server, SSH, runtime acquisition, or live
in-flight-operation interruption is established here. In-flight close semantics
are only covered by the separately labeled mock tier. There is no new cancel API
and no assertion of rollback after a broken pipe. Cached content uses a local
unreachable HTTPS URL only as an invalid fallback; a successful test must not
transfer bytes. World/modpack fixtures are unrelated to Java installer archives.

The dedicated workflow runs these same bounded checks on standard public Linux,
Windows, macOS ARM and macOS Intel runners. It uploads no binaries or packages.
A checked-in workflow is not evidence that those platform runs have passed.

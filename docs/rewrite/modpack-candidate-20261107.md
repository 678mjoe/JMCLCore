# Synthetic modpack normal-flow verification, 2026-11-07

The two existing selected `modpack_install` tests passed, each **1 passed /
0 failed / 0 ignored**, with 5 filtered out. No failure or permission denial was
observed. This evidence-only increment changes no production source, existing
test, fixture, dependency or workflow.

## Exact subject

- Executed clean local commit: `13c7044c0d095f17b904d0113e0b29d6a8a50ef9`.
- Exact tree: `e3479126c4665ec2975af43a7f0f7f2da85c2df3`.
- Corresponding preserved remote commit:
  `b98f0b13bbd677f8034d9d8b4d8e3c4ddb90b30c`.
- Linux x86_64 GNU, existing Rust/Cargo 1.99.1, default features, offline locked
  dependencies and an owned target/cache. A dedicated TMPDIR was empty before
  and after each test. The source checkout stayed clean throughout execution.
- Source and test executable fingerprints, exact commands, result counts and
  log SHA-256 values are in the
  [manifest](../../rust/compat/evidence/modpack-candidate-20261107/manifest.json).

Production and test source are unchanged from the preceding integrated
fmt/all-targets no-run/Clippy checks. Each command here also passed Cargo's
incremental compilation check before running its exact existing test. No full
runtime suite was selected or inferred.

## Source-reviewed execution boundary

The test helper creates a mrpack ZIP inside its own TempDir, containing a
synthetic index and optional `private` override bytes. The mod file download
body is the five literal bytes `hello`; its `.jar` filename does not make it
executable or a Java runtime archive. `Fake.fetch` checks the fixture URL,
counts calls and writes its in-memory body. It does not instantiate an HTTP
client, resolve DNS or contact `https://fixture/`. `NoProvider` would panic if
called; the selected Modrinth pack path does not call it.

The actual production path is `modpack::service::install` → local ZIP inspection
→ instance manifest creation → `Downloads::mrpack` → `install::execute_bytes`
→ bounded hash/size validation and CAS materialization → content registry →
private override replacement. A failed transaction removes only the newly
created instance while preserving verified shared objects. These calls do not
enter Java discovery/acquisition, native preparation, loader processors or a
subprocess. The generated mrpack and its overrides are deliberately processed;
this is allowed non-Java synthetic archive validation, not an archive-free test.

## Results

Commands share this prefix:

```sh
cargo test --manifest-path rust/Cargo.toml --offline --locked --test modpack_install
```

Append the exact name and `-- --exact --nocapture --test-threads=1`, in order:

1. `hardlink_content_private_overrides_and_cache_reuse`: the first install
   materializes verified `hello` bytes using the actual Linux shared inode.
   A second instance's `private` override leaves both the CAS object and first
   instance unchanged. The injected fetch counter remains one across both
   installs, proving this fixture's cache reuse.
2. `rollback_preserves_verified_cache_and_retry_has_no_new_format`: an owned
   pack containing `overrides/../bad` returns the expected `UnsafePath`, and the
   newly created instance is absent afterward. Replacing it with the valid
   owned pack succeeds, with total injected fetch calls still one. This is
   rollback/retry evidence, not a new format or an external traversal attempt.

These are production **service/API** tests, not native Core RPC, frontend,
real-provider or game/loader-install acceptance. There is no Java, processor,
real credential or external network operation in this slice. The finite results
do not establish arbitrary modpack/archive compatibility or cross-platform
parity and do not change `replacement_allowed=false`.

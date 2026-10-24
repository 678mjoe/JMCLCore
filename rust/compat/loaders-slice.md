# Forge/NeoForge processor slice

Oracle: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, `src/loaders/{neoforge_executor,forge_executor}.zig`.

`loaders::run` shares the client/server engine. The caller still performs policy-aware Java resolution **before** invoking it, even when cached output exists. The inner resolver prefers the requested major/architecture then any detected Java >= 8, exactly like the source. No Java download implementation is introduced here.

The chain is selected by side and stays in profile order. It reads bounded installer/JAR entries, substitutes literal/coordinate/embedded data, reads folded Main-Class manifests, runs literal argv with independent 16 MiB stdout/stderr limits, verifies the final optional Forge declared digest, publishes the generated artifact through the CAS and hard-links it back. Work directories clean up on error. State is atomic and written after CAS publication only.

**There is no per-processor `outputs` hash skip in the pinned implementation.** The whole-chain state shortcut validates recorded SHA-1 and size; it bypasses inner Java discovery, does not rerun processors, and does not independently compare the optional Forge declared SHA-1. Preserving these details is intentional. Legacy and processorless Forge are classified by metadata and do not call this engine; legacy server Forge remains unsupported in the original.

## Evidence

- 96 exact JSON record-order/field-order comparisons against the pinned Zig executor (4 client/server loader modes x 24 fixtures), plus matching output/state/order/work cleanup disk snapshots.
- Six Rust integration tests: corpus above, native synthetic success/cache, writer failure, cache with absent environment, actual Unix CAS inode sharing, missing environment and required-major fallback.
- All fixtures use generated tiny ZIPs and a benign native fake Java. No loader download, game, account, managed-Java archive extraction, or production instance is exercised.

```sh
zig build-exe --dep jmcl_core -Mroot=rust/compat/loaders_oracle.zig \
  -Mjmcl_core=/path/to/pinned/src/root.zig -femit-bin=/tmp/loaders-oracle
JMCL_ZIG_LOADERS_ORACLE=/tmp/loaders-oracle cargo test --test loaders_differential exact_processor_oracle -- --include-ignored
```

The differential tests are explicitly ignored by default. `--include-ignored` requires the named oracle environment variables and fails if absent; CI must supply them. Native macOS/Windows execution remains pending. Process descendants inheriting pipe handles can prolong completion, as in the pinned processor runner; no new process-group kill policy is introduced. Synthetic fixture process failure diagnostics are emitted to stderr, never RPC stdout. No completed-installation receipt is written by the engine itself: the caller owns it after processor success.

## Install RPC/CLI orchestration

Downloads complete first, then the caller resolves Java using the requested policy, then processors run and verify, and only then `.jmcl/install.json` is recorded. Legacy/processorless Forge plans bypass Java and processors. Existing local/installed-managed runtime policy remains available. Missing managed acquisition explicitly fails `JAVA_DOWNLOAD_FAILED` / `JavaDownloadFailed`; **no runtime archive installer is imported or exercised**.

- 20 CLI executions: exact stdout, stderr, exit and receipt (modern Forge/NeoForge, success/cache, missing output, wrong hash, incompatible Java, processorless Forge and legacy versionInfo Forge).
- 24 RPC executions: same scenarios plus failed process, with the single lifetime defect below isolated. Every unaffected event/result byte and receipt must match.
- Writer failure on processor progress aborts before process execution and leaves neither processor state nor installation receipt. Unavailable managed acquisition similarly leaves no success receipt.

### Explicit incompatibility: freed patched-client hash

Pinned `src/protocol/install.zig` lines 310/313 and 340/343 defer freeing `patched.patched_sha1` at the end of each loader block, while assigning that slice to `patched_client`. The final response serializes it **after the block has ended**. The Debug allocator poisons those 40 bytes as 0xAA, observed as `[170,...]` rather than a hash string. Rust retains the owning `Summary` through response serialization and emits the actual file hash. We deliberately do not emulate freed-memory reads.

The default RPC differential is strict and exposes the mismatch. Experimental CI may set `JMCL_ALLOW_PROCESSOR_HASH_LIFETIME_GAP=1`: this permits only exactly forty 170 bytes, requires Rust's hash to equal the generated file's SHA-1, and compares every other record and receipt. This is **not full wire parity**, and is not permission to change Zig main. User approval of a bounded defect-exception rule was pending when implemented.

The adapter prevents DNS/connect operations and all Rust fixture transports reject downloads. Build it as follows:

```sh
zig build-exe --dep jmcl_core --dep jmcl_cli \
  -Mroot=rust/compat/loaders_install_oracle.zig \
  -Mjmcl_core=/path/to/pinned/src/root.zig \
  --dep jmcl_core -Mjmcl_cli=/path/to/pinned/src/main.zig \
  -femit-bin=/tmp/loaders-install-oracle
JMCL_ZIG_LOADERS_ORACLE=/tmp/loaders-oracle \
JMCL_ZIG_LOADERS_INSTALL_ORACLE=/tmp/loaders-install-oracle \
JMCL_ALLOW_PROCESSOR_HASH_LIFETIME_GAP=1 \
  cargo test --test loaders_differential -- --include-ignored
```

Native Windows/macOS execution remains unvalidated by this Linux fixture evidence. Unix non-UTF-8 installation roots are exercised across all four engine layouts; OsString arguments preserve their bytes. The pre-existing managed-Java resolver still uses string store roots; non-UTF-8 managed-runtime-store lookup is outside this processor slice.

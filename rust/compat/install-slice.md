# Verified installer slice

This library slice implements immutable plan downloads, not a placeholder protocol
success. The integration layer owns request IDs, method envelopes, and stage events.

## Boundary and reuse

`install::execute` takes a validated `Plan`, anchored `ArtifactRoot`, `Store`,
`Options`, streaming `Transport`, and optional synchronous event callback. It emits
ordered file/retry/progress payloads. `HttpTransport` is HTTPS-only; tests explicitly
inject loopback or synthetic transports. Workers are bounded to 1–32 and retries to
0–5. The final retry alone uses fallback, matching the pinned executor. Successful
bytes transferred and retry-bounded bytes processed are distinct. Byte callbacks
occur at 256 KiB and aggregate progress is throttled at 100 ms.

`download_into_staging_file::<D, T>` provides reusable exact-size/digest validation
without publication. D is a digest 0.10-compatible implementation, e.g. Sha1 or
Sha256. The caller supplies a newly created empty file, owns its cleanup, sync and
atomic publication, and may interrupt through the byte callback. It never truncates
an existing file. This is suitable for private Java runtime staging without putting
mutable runtime trees into the immutable CAS. SHA1 behavior is tested here; a SHA256
caller must add/test its own sha2 implementation dependency.

## Hard links are the design, not an optional optimization

`Store::transaction` binds a private OS lock to the store and digest; its borrowed
staging object cannot outlive that transaction. It spans verify, invalid-object
removal, download, atomic publication, and anchored hardlink-first materialization.
There is no drop/reacquire window for concurrent GC. Auto-lock public CAS methods
must not be called inside a transaction. Descendants are resolved with Unix directory
FDs or Windows relative handles and no-follow rules. Failed staged writes are removed;
an invalid old linked inode is unlinked rather than overwritten in place.

## Evidence and remaining limits

Linux unit tests cover real inode sharing, cache reuse and reverse import, corrupt
and truncated/oversized downloads, final fallback, progress across retries, duplicate
digests, concurrent installers, GC exclusion, callback interruption and lock release,
unsafe paths and symlink descendants. The finite Zig differential compares exact
JSON events/results/errors and requested URL paths for cached/download/fallback,
short/oversized/hash/status cases using deterministic tiny loopback HTTP fixtures.
It does not prove real upstream availability or all transport/TLS error-name parity.
Large-stream timing interleavings are tested as invariants, not exact wall-clock
traces. Native extraction and protocol wiring are separate follow-up slices.

The Windows storage backend cross-compiles in an isolated storage-only crate for
x86_64-pc-windows-gnu. Full-crate cross-compilation needs a Windows C compiler for
reqwest's TLS dependency; native Windows execution remains a CI acceptance gate.

Build the pinned adapter (Zig 0.16, oracle revision
5a4310debbcee26b3d2391bab2339f79cd6ec52e):

```sh
zig build-exe --dep jmcl_core -Mroot=rust/compat/install_oracle.zig \
  -Mjmcl_core=/path/to/pinned-zig/src/root.zig -femit-bin=/tmp/install-oracle
JMCL_ZIG_INSTALL_ORACLE=/tmp/install-oracle \
  cargo test --test install_differential -- --include-ignored
```

## Native preparation follow-up

`install::prepare(&ArtifactRoot, &Plan)` returns `Preparation` with the legacy four
fields. It first projects assets hardlink-first, then preflights all native entries
and creates a unique `.jmcl/runtime/natives/<hex>` directory even when the extraction
set is empty. `cleanup_native` accepts only one such native-session path and removes
it without following descendants. Projection replacement and mutable overrides use
new private inodes; existing hardlinks are never truncated.

Preflight enforces 4096 entries per archive, 256 MiB per entry, 1 GiB combined output,
ratio 1000, safe portable paths, regular-file/directory entry types, prefix exclusions,
and duplicate native output rejection. Store/Deflate and ZIP64 metadata are parsed
without whole-archive buffering. Local-vs-central metadata is checked during extraction.
The raw decoder deliberately preserves the pinned extractor's lack of payload CRC
checks and exact-output-length behavior, including its absence of a compressed-size
read boundary. This is not a claim that those malformed-archive behaviors are desirable;
changing them requires the explicit defect policy. Source size/SHA1 is checked against
the plan, and extracted output is bounded independently.

Sixteen synthetic native archive cases compare errors, counts, extracted bytes and
normalized random directory fields to the pinned Zig adapter. Other tests prove
error cleanup, no-follow descendant safety, hardlink sharing, and private mutable
replacement. The corpus includes ZIP64 local/central/end records, the 4096-entry
limit even for excluded files, and first-error precedence across entries. Raw Unix ZIP filename bytes are preserved and covered by an oracle case; Windows
uses WTF-8-to-UTF-16 conversion with its own native test (execution awaits Windows CI). There is no real game, library loading,
or external archive acceptance claim.

```sh
zig build-exe --dep jmcl_core -Mroot=rust/compat/prepare_oracle.zig \
  -Mjmcl_core=/path/to/pinned-zig/src/root.zig -femit-bin=/tmp/prepare-oracle
JMCL_ZIG_PREPARE_ORACLE=/tmp/prepare-oracle \
  cargo test --test prepare_differential -- --include-ignored
```

Wiring additions: direct `flate2 = "=1.1.9"`, `getrandom = "=0.4.3"` dependencies.
`ArtifactRoot::absolute_path` and `absolute_directory` derive launch paths from the
held descriptor/handle (Linux procfs, macOS F_GETPATH, Windows final-handle path),
not a re-resolution of the original configured pathname.

The anchored ArtifactRoot bridge `from_directory(File)` is crate-private and expects
an already no-follow-opened directory. `atomic_replace(relative, bytes)` is for
mutable overrides and is not a CAS import operation.

## RPC and CLI wiring

`install::rpc::handle(out, id, method, params)` claims `install.execute` and
`install.prepare`; `install::cli::cli(args, out, err)` claims their CLI counterparts.
`handle_with`/`cli_with` inject a planner and artifact transport for finite offline
acceptance tests. Dispatcher registration remains integration-owned. Planning uses
current metadata `plan_loader`, `plan_neoforge`, and `plan_forge` APIs. Completed
installation receipts are schema 1 and atomically written only after successful
artifact execution and required subsequent stages.

**Scope gate:** Vanilla/Fabric and legacy Forge plans which need no processor chain
can complete this pipeline. Modern Forge/NeoForge execution returns an explicit
NOT_IMPLEMENTED error before filesystem work until the processor engine is wired;
it does not manufacture a successful terminal response or receipt. This is an
incomplete feature gate, not claimed protocol parity for those successful cases.

The pinned CLI `install prepare --forge-version ...` parses the option but omits it
from planning selection. This observable omission is retained and covered by a cached
fixture rather than silently corrected. RPC does include the Forge selection.

Tests compare 202 parameter-schema requests, cached successful execute/prepare for
both official and BMCLAPI sources, installation receipt bytes, 24 CLI result/usage
cases, and raw Unix RPC directory bytes. The metadata cache and tiny client file are
synthetic and local; no remote request is needed. Writer failure and the processor
gate are tested not to record completion. Dynamic native path values are normalized,
but JSON field order and other bytes are compared. The outer protocol layer owns
request-envelope/duplicate-member rejection. String-like byte arrays and numeric enum
tags follow the pinned parser; non-UTF8 opaque metadata identifiers are not covered
by this corpus (filesystem paths themselves preserve raw bytes/WTF-8).

```sh
JMCL_ZIG_ORACLE=/path/to/pinned/zig-out/bin/jmcl-core \
  cargo test --test install_rpc_differential -- --include-ignored
```

### Filesystem-dependent raw-name fixtures

APFS on the native macOS ARM runner rejected the existing `0xFF` fixture pathname
with EILSEQ before the cores were invoked. Raw-name tests now probe their temporary
filesystem, accepting only that observed capability rejection (not arbitrary IO
failures). On supporting filesystems the original byte-exact success assertions
remain. On rejecting filesystems both cores still receive the original raw RPC or
ZIP input: RPC errors and native error names must match exactly, and cleanup is
checked. Atomic byte-path replacement checks EILSEQ propagation and absence of leaked
temporary files; ordinary UTF-8 replacement and traversal checks always run.

Run these suites with `--include-ignored --show-output` to display any unsupported
success-branch reports. A rejection branch is not evidence that raw-name success
works on that filesystem. Linux success coverage can run locally; native APFS
rejection parity remains a separate CI gate. No OS error mismatch is waived by the
capability probe.

The separate production correction maps only Unix raw errno EILSEQ to
`BadPathName`, as pinned Zig 0.16 `std/Io/Threaded.zig` does in POSIX create/open
paths. It does not normalize OS error text or change unrelated errors. A Linux
synthetic errno regression proves this mapping; it is not a substitute for the
pending native APFS rejection/cleanup run.

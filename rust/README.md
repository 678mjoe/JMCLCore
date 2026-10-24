# Experimental Rust Core

This additive implementation is **not a replacement for the Zig Core yet**.
The executable is deliberately named `jmcl-core-rust`, while implemented wire
identity stays `jmcl-core`, protocol 1, version `0.1.0-dev`. No Zig delegation or
successful placeholder responses are used. Known unported methods return the
explicit development-only `NOT_IMPLEMENTED` error, which is a compatibility gap.

The pinned behavioral oracle is commit
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`. `compat/methods.json` distinguishes method names from the full source inventory;
[coverage.json](compat/coverage.json) tracks success, invalid/error/event/disk cases,
platform evidence, and known gaps separately. Neither method presence nor a green
finite corpus is proof of every parameter, filesystem or platform behavior.

## Build and checks

Use the pinned toolchain (`rust-toolchain.toml`) and committed dependency lock:

```sh
cargo test --locked
cargo build --locked
cargo clippy --all-targets --locked -- -D warnings
python compat/check.py --zig /path/to/pinned/zig-out/bin/jmcl-core \
  --rust target/debug/jmcl-core-rust --oracle-source /path/to/pinned/source
```

The oracle must be built from a separate checkout at the exact pinned commit;
the harness validates that checkout's HEAD and records both executable hashes.
It compares exit status, stdout and stderr exactly, without normalization.

### Known unresolved oracle defect

The pinned Zig RPC loop wipes its entire input buffer after discarding an
oversized line. If another request is already buffered, it is wiped too and
becomes a second invalid request. Rust preserves the subsequent request.
Three oversized-pipelined cases expose this mismatch. The strict harness fails;
`--allow-known-gaps` is a temporary, explicit experimental check which accepts
only the exact recorded pair of outcomes and reports each case separately with
`full_compatibility: false`. It never hides a new difference. The compatibility
policy/oracle repair decision is pending; do not claim replacement readiness.

Oversized EOF behavior, valid unterminated final lines, CRLF, duplicate fields,
Zig's numeric-string protocol coercion, and serial malformed-request recovery
are included in the foundation differential cases. Basic CLI help/version and
unknown-command/error cases are included. Additional offline instance, world/content, metadata/planning and Java CLI slices
are listed below; unsupported commands remain explicit gaps.

Request lines and read buffers are bounded and zeroized. Params stay borrowed
raw JSON; duplicate-key validation is iterative and does not materialize secret
values. Decoded owned request keys/identity fields are wiped on drop, including
partial-deserialization failures. This foundation does not yet implement auth
or claim an audited secret-memory guarantee for future handlers.

## First additive milestone

- Serial RPC framing, ping/version and basic CLI observations are compared to the
  fixed Zig oracle. Known oversized-pipeline divergence is explicitly unresolved.
- Hard-link CAS backends retain legacy digest paths, immutable publication,
  per-digest locking and conservative GC. Required native oracle tests compare GC
  and the actual shared OS lock, not just a matching filename. Linux CI requires
  a real cross-device copy fixture; other platform copy topologies remain separate.
- Public account records and offline instance operations use legacy formats and
  synthetic bidirectional takeover checks. Network instance validation remains
  unported. Server manifests/properties are codecs, not server RPC support.
- Full worlds/content/network/install/auth/launch/server lifecycle parity is not
  included in this milestone. The Zig executable and frontend pin are unchanged.

CI runs oracle-dependent tests explicitly with `JMCL_ZIG_ORACLE` and
`--include-ignored`; a developer run without that flag skips those tests and must
not be reported as an oracle compatibility pass. Cross-platform results are
recorded against the exact PR/main revision rather than inferred from Linux.


## Second experimental milestone

The isolated `rust-rewrite` branch now integrates native client world/NBT/archive
operations, local content/datapack registry mutations, source-aware metadata and
Vanilla/Fabric/NeoForge/modern-and-legacy-Forge plans, Java probing/selection and
existing managed runtime list/remove. The worlds remain mutable private copies,
not CAS hardlinks. No installer execution, runtime download, real game/server
launch or authentication completion is claimed here.

Linux combined tests explicitly execute oracle-marked cases using
`JMCL_ZIG_ORACLE` and `JMCL_ZIG_METADATA_ORACLE`; build the adapter as described
in [version-slice.md](compat/version-slice.md). The raw metadata corpus contains
106 network-free RPC/CLI cases, and Java contains 59 raw RPC/CLI/disk checks.
Archive interoperability compares restored content, metadata and actual size,
not byte-identical compressor output. CRC acceptance remains a documented gap.

Native Windows and both macOS architectures must execute the combined milestone
in CI. Prior individual crosschecks are not combined native acceptance; the
reqwest/AWS-LC dependency additionally requires a C crosscompiler for Linux-to-
Windows crosschecking. No TLS validation bypass is used.
### Windows pinned-oracle lock contention

Native CI confirmed that pinned Zig GC exits 1 with exactly one collection error
while Rust holds the eligible object's digest lock; the object is retained. A
fresh Zig GC after release exits 0 and deletes it. This establishes exclusion,
**not blocking parity**. Source inspection suggests a cause: Zig's existing-file Windows lock path uses an
asynchronous handle but does not handle pending completion; the native NT status
was not directly traced. Neither oracle nor lock behavior
is changed here; the compatibility decision remains pending.

Strict `store_differential` rejects this difference. Experimental CI explicitly
sets `JMCL_ALLOW_KNOWN_ORACLE_GAPS=1` and accepts only the exact observed exit/status/
stdout/stderr/retained-object and post-release pair, on Windows only. Unexpected
differences fail. CI prints the known-gap reason; this is additional to the three
oversized-input cases, not a replacement-readiness pass.

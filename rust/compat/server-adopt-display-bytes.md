# Server adoption RPC display-name consumer

This second stage changes only the dedicated adoption RPC arm: `load_read` plus
`open_server` replaces the operational `target_from_root(false)` call. The same
manifest validation precedes the unchanged lifecycle guard and datapack world
handling. Adoption does not consume name/version/loader values; no fake values or
lossy conversion are introduced. Operational target helpers, CLI adoption,
provider paths, filesystem, CAS transactions and lifecycle remain byte-identical.
CLI adoption is a separate remaining consumer, not implicitly fixed by this RPC.

Base target stage: da93c8edde7472abba202bb8de7f4dbbffb877d8.
Pinned oracle: 5a4310debbcee26b3d2391bab2339f79cd6ec52e.
Its focused new unit was red: invalid stored ID combined with opaque/empty name
returned ADOPT_FAILED instead of SERVER_NOT_FOUND. After this narrow bridge,
the unit is green, and 126 paired sequences / 504 calls have zero wire differences
and no direct-state invariant failures (baseline 20 mismatching sequences).

The separate metadata matrix varies String/ASCII/Unicode/opaque/control names,
empty/long/overflow/null inputs, missing required version, invalid stored ID,
ID mismatch, schema and source errors. Every request has an explicit nonempty
owned store; preinitialized CAS and guard paths prevent incidental setup effects
from hiding failed-call mutations. No server.pid is present before or after calls.
Success and repeat behavior assert raw registry bytes, same immutable CAS/content/
peer inode and bytes; failures require unchanged full raw snapshots. No archive
interpretation, lifecycle contention, process identities, network or Java occurs.

All 10 canonical source/Rust layout differences remain printed and separately
asserted under accepted COMPAT-CAS-001. The only expected CAS layout difference is
Rust's persistent empty digest-lock leaf; this is not whole-disk source parity.
The independent GC test's limited concurrency evidence is not expanded here.

```
cargo test --test server_adopt_display_bytes --test server_adopt_target_bytes
python rust/compat/server_adopt_display_bytes.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Non-Linux fixtures check only existing platform gates. Local Linux results do not
substitute for native acceptance. The same physical worktree and low-debug target
were rebuilt sequentially with incremental compilation disabled; stage-one
executable and SHA were retained before this source change.

Regression evidence: all 72 target sequences / 320 calls remain exact at the wire
(with all 25 accepted layout differences retained); 756 prior local mutation
RPC pairs remain exact. All-target Clippy, formatting and diff checks pass.

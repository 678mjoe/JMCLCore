# Server adoption target bytes (RPC only)

The dedicated `server.mods.adopt` / `server.datapacks.adopt` arm retains byte
parameters until their original filesystem stage. It uses existing target and
dynamic-u8 decoders, not a new global Params interpretation. Ordering is complete
parse, nonempty directory/ID, CAS open, server root, ID/manifest, server directory,
stopped guard, datapack properties/derived world, then ordinary opaque adoption.
The operational manifest consumer is intentionally unchanged in this stage;
stored display-name byte handling is a separate known consumer issue.

All store/transaction/lifecycle/world internals, CLI and provider paths remain
unchanged. An absent store parameter still selects the same DEFAULT_DIRECTORY;
explicit null remains invalid. This default equivalence is source-reviewed only:
no fixture executes an absent/null/default/empty store. Every filesystem request
has an explicit nonempty owned store, including malformed-byte parse controls.

Pinned source: 5a4310debbcee26b3d2391bab2339f79cd6ec52e.
Baseline: 1a5a66c / 6331eea1, binary
c4c1ab1e03c41d13ec122590611b446f7b35b7d83094c22372beb4733c7d0bc5.
The baseline matrix has 72 paired sequences / 320 total calls (including 16 Rust
String twins), with 34 wire differences. Inputs cover individual target/store
arrays, Unicode and supported opaque paths, dynamic numeric grammar, range and
ID errors, blocked CAS/root combinations, stored metadata, guard/property errors,
repeat adoption and separately labeled fresh-store/derived-world creation.

Every fixture has no server.pid, a precreated unheld guard or explicitly malformed
guard leaf, opaque content bytes and a readonly outside hardlink peer. No process
matching, server launch, held lifecycle contention, acquisition or archive parsing
is exercised. Error snapshots preserve the game; fresh-store errors permit only
the source-ordered CAS scaffold. Success directly checks raw registry bytes,
content/peer/CAS shared identity, SHA1, readonly state and repeat results.

COMPAT-CAS-001 remains explicit: each engine has an independently asserted exact
CAS tree. Source adoption leaves no digest lock; Rust retains exactly one empty
regular leaf with stable identity. Raw per-engine snapshots and all canonical
layout differences remain visible; no whole-disk source parity is claimed. UTF8
byte paths have full Rust String twins; opaque paths have no false String twin.
The separate deterministic GC test establishes stale-preflight rechecking on
Linux, not universal concurrent safety or native Windows/macOS acceptance.

```
cargo test --test server_adopt_target_bytes
python rust/compat/server_adopt_target_bytes.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Non-Linux execution checks the existing platform gate only. No CLI/raw argument,
provider/network, Java, archive or other server operation is included.

Local Linux result: focused range-order unit red→green; 72 sequences / 320 calls
now have zero wire mismatches with all direct invariants passing. All 25 canonical
layout differences remain explicitly printed and independently asserted under
COMPAT-CAS-001. Prior local server target corpus: 456 exact RPC pairs unchanged.
All-target Clippy and formatting/diff checks pass. Native gates remain separate.

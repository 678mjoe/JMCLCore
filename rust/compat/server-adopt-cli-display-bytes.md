# Server adoption CLI display-name consumer

Only ordinary-argument `server mods|datapacks adopt` joins the existing CLI-local
read-view branch. The helper returns genuine version/loader fields, while adoption
uses neither; operational provider target helpers are unchanged. CAS opens before
the server target, then metadata, stopped guard, properties/world and opaque local
adoption retain their existing order. No new parameter decoding or raw OS argument
route is added. Every executed command includes an explicit nonempty owned
`--store-directory`; no default/absent/null/empty-store path is exercised.

Pinned source: 5a4310debbcee26b3d2391bab2339f79cd6ec52e.
Baseline central source cca6765 / tree f6618bf2, binary
19ca5fe0316a230d1284d1c2283143658a4560f331969e18cb70a38ff00078e8.
Full initial evidence: 150 paired sequences / 600 total calls, with 112 mismatching
sequences and no state invariant violations. These contain TWO distinct issues:

* 104 sequences concern the persisted display-name consumer and its error order.
* 8 sequences concern blocked explicit CAS roots: source prints `NotDir`, Rust
  prints `PathAlreadyExists`. This occurs with ordinary String names too, before
  manifest reading. It is not fixed by this change and is NOT a COMPAT-CAS-001
  allowance. The full original raw evidence is retained separately.

The consumer-only gate names these 8 exclusions explicitly and covers the other
142 paired sequences / 568 total calls. Never describe it as all 600 calls passing.
The root-error issue remains an independently actionable shared-store task; no
comparison is normalized to hide it. Cases combine names (String/ASCII/Unicode/
opaque/control/empty/long/overflow/null), metadata errors, blocked server root,
malformed guard and property-order controls. Each has no server.pid before/after,
a precreated unheld guard or malformed leaf, opaque content and immutable peer.

Success/repeat output is asserted exactly, including adoptedCount; raw registry
bytes, CAS/content/peer identity, readonly state and content hashes are checked.
Every failure preserves a full snapshot. Per-engine CAS trees are asserted
independently and raw layout differences retained under COMPAT-CAS-001; no
whole-disk source parity or general concurrency proof is claimed.

```
cargo test --test server_adopt_cli_display_bytes
python rust/compat/server_adopt_cli_display_bytes.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Non-Linux execution checks the existing platform gate only. No live process
matching, signals, provider access, Java or archive interpretation occurs.

Local Linux result: focused unit red→green; the explicitly bounded 142-sequence /
568-call consumer corpus has zero response/invariant failures, with 12 separate
COMPAT-CAS-001 layout differences retained. The 8 root diagnostic sequences remain
open at this stage. Prior local CLI 1,232 pairs plus 80 argument/target pairs stay
exact. All-target Clippy and formatting/diff checks pass; native gates are pending.

The subsequent [root error fidelity slice](store-root-error-fidelity.md) restores
those eight cases to the checked-in corpus: 150 sequences / 600 calls. The
name-only historical result above is not the current unresolved-case count.

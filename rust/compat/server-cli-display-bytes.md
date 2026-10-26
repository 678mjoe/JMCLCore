# Server local CLI display-name bytes

Scope: `server mods|datapacks list|enable|disable|remove`, ordinary String
arguments only. The private CLI-local read target helper uses the existing
`load_read` view while returning the genuine stored version and derived loader;
no placeholder operational values or lossy name conversion are introduced.
Operational target helpers and install/set-version/adopt execution are unchanged.

CLI argument/ID validation and exactly-one-selector errors precede root opening.
Then root, manifest, server directory, mutation-only stopped guard, datapack
properties/world, and local registry/selector handling retain their old order.
LIST takes no lifecycle guard. Each fixture asserts server.pid absent before and
after calls, so no live process matching is reached. Mutations use a precreated
unheld guard, opaque readonly content, and an owned external hardlink peer.

Pinned source: 5a4310debbcee26b3d2391bab2339f79cd6ec52e.
Baseline: 326913eb, binary SHA256
`de92da6d5d1a054018281c93d09f4d880efe1f4a13ed600984a345ab6c47b138`.
The baseline has 548 differences in 770 paired sequences / 1,232 CLI calls per engine;
plain String controls already match. CLI exposes underlying typed/semantic errors,
so the matrix checks missing fields, invalid stored IDs, schema/source, array
numeric errors, invalid names and guard/property/selector ordering, not merely
success versus failure. LIST JSON and text output are both checked.

All response pairs are captured before comparisons. Every failure and LIST call
requires an unchanged full snapshot; successful mutations assert exact registry
bytes, paths, readonly shared identity/bytes and repeat behavior. Raw per-engine
snapshots include inode/device/mode/mtime/nlink/bytes; cross-fixture comparisons
use only registry bytes, relative path presence and link count, not inode numbers.
The additional 80 prior local CLI argument/target controls are restricted from the
old mixed suite to these four operations; no install/adopt/provider call is run.

Run the named `server_cli_display_bytes` test and:

```
python rust/compat/server_cli_display_bytes.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Non-Linux runs check only the eight existing platform gates without fixtures;
this does not establish native server support. No raw OS argument decoding, CAS,
Java, archive interpretation, process identities/signals or network access occurs.

Local Linux evidence: the focused unit was red on the exact baseline and green
after the helper change. All 770 paired sequences / 1,232 CLI pairs then matched
with zero invariant violations, plus all 80 prior local argument/target pairs.
Only the dedicated local scopes were executed; no mixed provider/adoption suite.
All-target Clippy and formatting/diff checks passed on this local Linux tree.

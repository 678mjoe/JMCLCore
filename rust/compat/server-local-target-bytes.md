# No-state server local content target bytes

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e` accepts dynamic byte strings
for `ServerContentSelectorParams.directory/id`. The six local enable/disable/remove
methods check range and exactly-one-selector, then open root, validate/load the
manifest, open the server, acquire lockStopped, resolve the datapack properties/
world if applicable, and only then validate the selector and mutate the registry.

A dedicated six-method RPC arm reuses LookupTarget/native-root decoding and the
**unchanged operational** target_from_root helper. The existing lifecycle guard
and world selection are unchanged; guard lifetime covers selection/response as
before. Existing remove/toggle execution is extracted into selected_fields and
shared with the prior local path. No sentinel Params or lossy path conversion is
used. Existing Params/network/adoption/CLI parsers, manifest-loading path, lifecycle
implementation and store files remain byte-identical. LIST remains its separate
read-view consumer. Opaque operational manifest display names are not widened.

Validation:

```
cargo test --test server_local_target_bytes
python rust/compat/server_local_target_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

Linux evidence:228 paired fresh no-state sequences /456 RPC pairs. The prior tree
has129 differences; current wire/raw-registry/full relative tree effects match.
The actual blocked-root unit is red with INVALID_PARAMS before the fix and green
with SERVER_NOT_FOUND afterward, without creating any server metadata.

Positive controls explicitly require successful operations and repeat outcomes;
error equality alone is not a pass. Cases include String/ASCII/Unicode/native
root arrays, byte IDs/numeric grammar, malformed IDs/elements, blocked roots,
missing/invalid/mismatched manifest fields, malformed guard leaf, selector range
and validation, bad server properties, repeats and absent physical files.
Precreated unheld lifecycle locks avoid incidental guard creation. Every fixture
asserts `.jmcl/server.pid` absent before/after; both source status routines return
stopped before process matching in that case. No PID or identity content, signals,
held lifecycle contention, server start/stop, acquisition, CAS, Java or archives.
Payloads are opaque owned files with a readonly shared hardlink peer.

Every failed call preserves its complete before/after snapshot. Every post-call
raw snapshot retains actual inode/device/mode/mtime/nlink/file bytes per engine.
Cross-fixture comparison separately covers every relative path, exact raw bytes,
mode and link count; inode/time identities are asserted within each fixture,
not incorrectly equated across independently created trees. No file or lifecycle
lock artifact is filtered out. Peer bytes/inode/mode/mtime and sibling content are
preserved; only intended content rename/unlink/registry changes are permitted.

Physical raw FF directories exclude only exact EILSEQ with an explicit capability
record. Non-Linux invocations check all six exact UNSUPPORTED_PLATFORM gates before
parameters, without filesystem fixtures or lifecycle access. Local execution is
Linux-only; gate preparation is not Windows/macOS mutation acceptance.

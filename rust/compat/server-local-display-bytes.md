# Server local mutation display-name validation

The six RPC methods `server.{mods,datapacks}.{enable,disable,remove}` now validate
stored metadata through the existing byte-preserving `load_read` view before
opening the server directory and taking the unchanged stopped-server guard.
They do not need operational display-name Strings. The operational
`target_from_root`, CLI/network/adoption paths, lifecycle and store are unchanged.

Pinned source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
Base: `2eb573e`. The actual new unit is red on that base: invalid stored ID plus
opaque name returned toggle failure rather than `SERVER_NOT_FOUND`.
The frozen preceding binary's 378 paired sequences had 60 mismatches.
After this change, 378 sequences / 756 RPC pairs match exactly, including valid
String/ASCII/Unicode/opaque/control names; empty/long/overflow/null names;
missing version, invalid stored ID, ID mismatch, schema and source errors.
Stored invalid ID precedes semantic name validation, while missing typed fields
still fail before semantics. Prior target corpus: 228 sequences / 456 pairs green.

Every successful mutation asserts registry bytes, path state, repeat behavior,
and an external owned hardlink peer's immutable bytes/identity/mode. Every
failed call requires an unchanged full snapshot. Per-engine raw snapshots
include inode/device/mode/mtime/nlink/bytes for diagnostics; only registry bytes,
relative path presence and link counts are compared across different fixtures.
Fixtures precreate an unheld lifecycle lock and assert server.pid absent before
and after each call. No live process identity, signal, installation or provider
access is exercised. Linux was executed locally; non-Linux harness execution
checks the existing platform gate, not native server operation support.

Run `cargo test --test server_local_display_bytes --test server_local_target_bytes`
and `python rust/compat/server_local_display_bytes.py --zig <pinned-binary>
--rust <rust-binary> --oracle-source <pinned-source>`.
All-target Clippy passes. No native acceptance or whole-server parity claim.

# Server get: named world and icon aliases

Pinned Zig `5a4310de` follows the explicitly selected world directory and
`icon.png` aliases in `src/worlds/worlds.zig:231–247` (`readIcon`). Its server
RPC caller, `src/protocol/worlds.zig:935–941`, maps **all** icon lookup/read
errors to null after world metadata succeeds.

The private Rust server read handler now uses the existing
`child_alias_native` / `read_alias` helpers for that one get-icon expression.
The complete lookup is converted to optional data, retaining the 65,536-byte
limit. Metadata and `has_icon` are unchanged. Missing/broken worlds still
fail before icon handling; a valid world with an unreadable, oversized,
broken or directory icon still returns metadata with a null icon payload.
This does not change recursive traversal, filesystem helpers, CLI, NBT,
mutation/archive behavior, or the platform gate.

`server_world_icon_aliases.py` checks 13 exact Linux RPC outcomes: regular,
empty, missing, exactly-at-limit and oversized icons; named icon/world/both
aliases; missing icon under a world alias; broken icon/world, directory icon,
and a regular file instead of a world. Positive metadata, `has_icon`, exact
base64/null and the two earlier world-error codes are asserted. Both cores
must preserve all owned paths, outside alias targets and readonly hardlinked
peers (bytes, modes, mtimes, inode and link identities). Fixtures contain no
process state, executables or cleanup-candidate names. No lifecycle, mutation,
archive, Java or network operation runs. Other platforms assert the existing
unsupported-platform RPC response, not successful alias execution.

Before the fix, four of 13 cases differed: icon aliases returned null, and
world aliases failed during the no-follow icon child lookup. All 13 pass on
Linux after the fix; the 81 parameter and 138 server read metadata regressions
also pass. Exact-tree native CI remains separate evidence.

```sh
python rust/compat/server_world_icon_aliases.py --zig "$JMCL_ZIG_ORACLE" \
  --rust target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
```

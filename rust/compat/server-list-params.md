# Server listing parses only its directory field

Pinned `src/protocol/server.zig::writeListResult` uses a directory-only schema
and ignores unrelated fields. Rust previously built a complete `serde_json::Value`
before selecting fields; an otherwise irrelevant `1e400`/`1e9999` therefore
rejected the whole request. Ordinary well-formed unrelated lifecycle fields
already matched and were not the bug.

Linux `server.list` now has its own raw directory-only view. The existing
anchored enumeration, limit, ID ordering and installation-status reads are
moved unchanged into this read-only entrypoint. The shared operational/lifecycle
Value decoder is not altered. Directory byte arrays are handled by the separate
[checked native path bridge](lookup-directory-bytes.md).

```sh
python rust/compat/server_list_params.py --zig "$JMCL_ZIG_ORACLE" \
  --rust rust/target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
```

24 exact pinned comparisons pass on Linux: ignored huge values, nested ignored
numbers, recognized-but-irrelevant fields, malformed/missing/empty directory,
blocked root, object shape, ordered positive listing and complete preexisting
file/hardlink snapshots. A separate owned missing-root fixture verifies baseline
root creation followed by an empty successful list. No mutation, lifecycle,
installation or Java methods are invoked. Native CI on other hosts must retain
the original unsupported-platform response; it is not server execution evidence.
The outer duplicate-field `INVALID_REQUEST` controls are covered separately by
[lookup-id-bytes.md](lookup-id-bytes.md).

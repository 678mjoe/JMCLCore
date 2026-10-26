# Byte-preserving manifest display names

Pinned Zig schemas (`src/instance_store.zig`, `src/server/server_store.zig`)
store `name` as `[]const u8`, not a Unicode-only string. Typed JSON accepts a
string or an array of typed u8 values. Name validation checks byte length
1–256; it does not reject embedded NUL, TAB/LF or opaque bytes. Zig serializes
valid UTF-8 bytes as a JSON string, otherwise as a numeric array. Text CLI
writes the original bytes, including controls. These are compatibility rules,
not new terminal escaping or general filename policies.

## Read-only boundary

Private `ReadManifest` types serve `instance.get/list`, `instance show/list`,
and the Linux-only server equivalents. `DisplayName` retains bytes and uses
the existing exact typed-number helper for array elements. Public operational
`Manifest` fields remain `String`; launch, install, create/delete and lifecycle
consumers are not converted to a new representation. Ordered semantic
validation is shared through borrowed fields, not a placeholder or lossy name.

The instance filesystem adapters share their original anchored loader and
sorted enumeration. Existing manifest leaf-alias behavior and limits are
unchanged. Installation status uses a borrowed identity without any display
name; server source comparison and the instance's intentional absence of source
comparison remain unchanged. Prepared marker/file fixtures test both rules
without executing installation or Java.

## Evidence and boundaries

```sh
python rust/compat/manifest_display_names.py --zig "$JMCL_ZIG_ORACLE" \
  --rust rust/target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
cargo test --manifest-path rust/Cargo.toml --lib display_name_tests
```

The Linux corpus compares exact exit status, stdout and stderr for 458 paired
RPC/CLI observations against pinned `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
It separately requires successful expected names/statuses, raw control bytes,
ID-sorted mixed names, typed-u8 grammar/precision/errors, empty and byte-length
boundaries, parse/semantic ordering, installation identity, and supported
manifest leaf aliases. Complete owned-tree snapshots include bytes, inode,
mode, mtime and link count, with unrelated hardlinked files preserved. Two
additional focused Rust tests protect the unchanged operational String model
and shared validation. No mutation, lifecycle, discovery or Java RPC is used.

Native macOS/Windows execution remains a CI gate. Non-Linux server commands
must retain their existing unsupported response. Unix leaf-symlink fixtures
are explicitly not exercised on Windows; no privilege availability is assumed.

This closes **human-name read presentation only**. Other typed byte-slice fields
(IDs, version/loader strings, Java paths) and opaque-name operational consumers
remain separate compatibility work; no full manifest-byte compatibility is
claimed. The two previously documented malformed JSON first-error cases in
[manifest-source-enum.md](manifest-source-enum.md) remain outside this change.

# Byte-preserving world enumeration on the read-only RPC view

Pinned `src/worlds/worlds.zig:159–183` enumerates directory entries as byte
names. It skips non-directories, temporary-marker names and invalid names;
checks the accepted-world limit **before** inspecting the next valid directory;
suppresses an individual world inspection error; and finally sorts accepted
records by byte order. Missing/malformed level metadata does not suppress the
world: metadata fields become null.

The previous read view converted each directory name to UTF-8 text and dropped
opaque names. Actual Linux proof found six mixed worlds versus Rust's four;
1023 ASCII + one opaque world became 1023 rather than 1024; and 1024 ASCII +
one opaque world incorrectly returned success instead of the source's
`WORLD_LIST_FAILED` limit result.

Only private `worlds/read.rs` changes. Native names become exact Unix bytes or
UTF-16-derived WTF-8 on Windows. A private collector preserves source iteration,
limit and inspection-error order, then byte-sorts accepted records. Existing
read-view serialization emits valid UTF-8 names as strings and other byte names
as numeric arrays. Public String functions, CLI, mutation/archive code and all
filesystem primitives remain unchanged.

```sh
python rust/compat/world_list_names.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/checkout-at-5a4310d
cargo test --locked --lib worlds::read::tests
```

The finite corpus compares exact full RPC outputs, expected counts/order/name
bytes, metadata fallback and complete unchanged tree snapshots. Unicode cases
always run; physical opaque-name mixed/count cases run only if the filesystem
supports them. Only EILSEQ / Windows ERROR_INVALID_NAME capability rejection is
recognized, explicitly not an opaque-name success. Windows uses actual lone
surrogate WCHAR names and exact WTF-8 expectations, not replacement strings.
Fixtures contain no cleanup-candidate names. Temporary-marker filtering is
covered only by pure predicates; no cleanup or archive operation is tested.
Private collector unit controls prove valid-entry count checking precedes the
next read, failed inspections do not count, and invalid/temp/non-directory
entries do not trigger the full-list error.

This closes opaque enumeration for the read-only RPC view only. Existing
String CLI enumeration and raw temporary-name cleanup remain separate consumer
boundaries; no general all-worlds byte parity is claimed. Native platform
execution remains pending CI. No lifecycle, Java, acquisition, archive,
network or actual game data is involved.

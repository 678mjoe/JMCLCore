# Persisted public-account byte strings

Pinned Zig `src/auth/account_store.zig` parses `DiskAccount` directly with
`std.json.parseFromSlice`, whose `[]const u8` fields accept JSON strings and arrays
of typed u8 values. Rust previously decoded those four fields as strings only.
Valid legacy metadata could therefore disappear from the Rust list and fail get,
without the record itself being corrupt. An initial read-only corpus found eight
such differences among 24 fixtures.

The disk-only decoder now accepts arrays using the existing typed numeric helper.
All account identifiers, player names, client UUIDs and XUIDs still pass the
existing account validation. Those grammars admit only ASCII, so non-UTF8 arrays
cannot become valid accounts; this does not introduce opaque output encoding.
Schema validation, duplicate known fields, ignored unknown fields and manifest
limits remain unchanged. Optional XUID retains absent/null behavior.

## Evidence

`compat/account_disk_bytes.py` uses exact pinned source
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` and executable arguments. It runs 29
synthetic persisted fixtures through **get/list only**, comparing complete stdout
and independently asserting valid decoded metadata or exact rejection plus list
omission. Each fixture has an unrelated valid sibling and an external hardlink.
Before and after each Core invocation, all directory/file entries, file bytes,
inodes, link counts, modes and modification times must match. Access times are
not compared because reads can legitimately update them.

Cases include all four array-form fields; quoted, float and binary128 boundary
u8 values; malformed elements; non-ASCII and forbidden ASCII values; unknown and
duplicate fields; missing fields; and mismatched IDs. Only fake public metadata
is used, never tokens, authentication, save/delete or real account data. Linux
local evidence passes; macOS/Windows execution of this added corpus awaits CI.

```sh
python rust/compat/account_disk_bytes.py \
  --zig /path/to/pinned/zig-core --rust /path/to/rust-core \
  --oracle-source /path/to/pinned/source
cd rust
cargo test --locked --lib accounts::disk_byte_tests
```

## Separate parameter evidence

A separate four-case read-only audit found three mismatches: array-form `get.id`,
quoted-u8 array `get.id`, and array-form `list.directory` are accepted by Zig's
`src/protocol/auth.zig` `parseFromValue`, while Rust currently rejects them.
Overflow-byte rejection agrees. That dynamic RPC path has binary64 behavior and
must not reuse the disk's binary128 numeric interpretation blindly. This patch
leaves parameter parsing unchanged and does not exercise save/delete; its regression
also asserts that the public `Account` decoder still rejects arrays. The subsequent
[parameter-boundary correction](account-param-bytes.md) uses separate private types
and separate dynamic numeric grammar.

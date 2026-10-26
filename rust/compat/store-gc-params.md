# Store GC request parameter bytes (pure preparation)

Pinned `src/protocol/store.zig` decodes a dynamic JSON value into a byte-string
`store_directory`, `dry_run=true`, and dynamic u64 `min_age_seconds=86400`.
Absent params use the same defaults. Complete typed decoding precedes the
empty-directory guard; opening an existing store and collection happen later.

A behavior-preserving extraction exposed Rust's actual preparation boundary.
The pure baseline contained 73 normal cases and four separate outer duplicate-key
controls. Eleven normal outcomes differed: empty byte arrays received the wrong
error message, and ASCII, Unicode, opaque, integral-float and quoted-integer byte
arrays were rejected rather than decoded. Age/default/bool controls agreed.
Neither this evidence nor the tests invoke a handler, open a Store, or collect.

## Narrow private RPC view

CLI `Params`, its String path/defaults, and CLI parsing are unchanged. Private
`RpcParams` retains directory bytes through complete decoding and the existing
empty guard. Byte elements use the existing dynamic-u8 helper; raw binary64 and
quoted numeric spellings retain their distinct semantics. The existing age
helper remains unchanged.

Only at the original root-opening stage does `prepared_root` call the existing
platform `path_from_bytes`. Conversion errors use the existing `STORE_ROOT_ERROR`
message. `Store::open_existing`, collection, no-follow/locking and filesystem
algorithms are unchanged. Unix byte preservation and Windows checked WTF-8
conversion are reused, not reimplemented. Conversion is not root acceptance:
for example NUL can exist in an OsString but fail the later unchanged open.

## Pure executable boundary

Run only these named tests:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  store::commands::params_tests::pure_gc_params_and_native_conversion -- --exact
JMCL_ZIG_SOURCE=/path/to/pinned/core \
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  store::commands::params_tests::pure_gc_params_match_pinned_schema \
  -- --exact --ignored --nocapture
```

The checked-in adapter imports only Zig `std`. The original struct and parse
options/empty messages are bound to reviewed normalized source bytes; the literal
`.jmclcore` default is checked against the original constant without importing
an operational module. The fixture reproduces a source-derived empty guard;
it does not execute an original handler. Positive decoded values and error
code/message are exact. Four duplicate-key rows explicitly record original outer
Value `DuplicateField`, distinct from Rust helper rejection; both dispatchers
reject duplicate keys before calling this helper. They are not counted as inner
parity or handler differences.

The corpus includes 121 strict parameter cases plus those four outer controls.
Owned input bytes, modification time, readonly state and Unix file identity are
checked unchanged. Platform conversion checks never access the named paths.
No Store/GC/CLI/handler, archive, Java, transport or network operation is executed.
Native CI and actual root/collection acceptance remain separate evidence gates.

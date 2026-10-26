# Persisted manifest source enum grammar and precedence

The instance and server manifests use Zig's typed `DownloadSource` enum, not an
unconstrained string. Invalid enum tags must fail during parsing, before semantic
name/version/ID validation. Rust's old String decoder deferred source validation,
so an invalid source combined with an empty name reported the wrong error.

A small shared source deserializer keeps the public String API and emits the
existing `Zig:InvalidEnumTag` parser detail. It also preserves the less-obvious
pinned grammar: `static.zig` `sliceToEnum` accepts integer-shaped ordinal tokens
or strings (0 = official, 1 = bmclapi), including legal signs/underscores.
Floating-point coercion is not allowed for enums. Exactly `-0` is specifically
not integer-shaped under Zig's scanner, unlike `-00`; both cases are tested.
Canonical strings remain the serialized representation.

Only instance/server manifest source fields use this helper. Defaults, direct
construction validation, RPC envelopes, dynamic create parameters and installation
record parsing remain unchanged. No general string/byte representation changes
are included.

## Evidence and invocation

`compat/manifest_source_enum.py` runs 43 fixture conditions for each manifest,
through text show, JSON show, get and list: 344 exact comparisons on Linux. It
asserts success values independently and checks complete disk snapshots. Cases
cover valid/default tags, numeric ordinal grammar, invalid token kinds, competing
semantic errors, missing fields, duplicate order, numeric overflow order and a
truncated document with a valid enum. All inputs are owned synthetic metadata;
no creation, lifecycle, process-state, Java, download or archive operation runs.

Portable pure units exercise both manifest parsers. Linux local tests pass;
macOS/Windows execution awaits native CI. Non-Linux server RPC/CLI tests assert
only the existing unsupported-platform gate; instance tests still exercise the
actual manifest paths there.

```sh
python rust/compat/manifest_source_enum.py \
  --zig /path/to/pinned/zig-core --rust /path/to/rust-core \
  --oracle-source /path/to/source-at-5a4310d
cd rust
cargo test --locked --lib instances::manifest_enum_tests
```

## Ordered syntax follow-up

The two originally deferred cases (`{"source":"bad",` and an invalid later `!`
token) are covered by the private four-manifest ordered schema described in
[manifest-ordered.md](manifest-ordered.md). It also preserves earlier malformed
ignored strings ahead of later enum errors; a typed-serde-first reorder alone
would not be sufficient. Public serde construction and encoding remain unchanged.
The separately recorded oversized server CLI diagnostic is not part of that
parser correction. General operational byte-string representation remains separate.

# Persisted registry typed enum grammar

Pinned `src/content/content_registry.zig` defines `Kind` and `Provider` as
four- and three-value enums. Zig 0.16 `std/json/static.zig::sliceToEnum` first
matches a declared name, then accepts an integer-formatted value parsed into
the unsigned enum tag type. It does not apply floating-point integer coercion.

A finite pure registry audit found eight valid persisted enum spellings rejected
by Rust: raw ordinal `0`, and strings `"0"`, `"+0"`, `"0_0"`, for both fields.
The actual `registry::parse` regression failed before the change. Out-of-range,
negative-zero and float forms also had incorrect error names.

Only `WireEntry.kind` and `WireSource.provider` now use private typed enum
decoders. Public `Kind`/`Provider` serde implementations are unchanged and remain
named-only. Dynamic RPC decoding, registry serialization and semantic validation
are unchanged.

The private decoder checks names first, then rejects negative values (including
negative zero), decimal points and exponents before invoking the integer branch
of the existing spelling helper. Both tag types fit two bits; `Provider` also
rejects ordinal 3, which has no declared value. The private ordered parser now returns `InvalidEnumTag` directly, without
normalizing unrelated errors.

## Evidence

`content_registry_enums` compares all declared variants in named, ordinal and
numeric-string forms, invalid signs/spellings, float/range/unknown values,
wrong types, duplicates and typed error precedence. A separate test verifies
that public serde enums still reject numeric forms. Inputs retain bytes,
modification times and readonly state, and Unix device/inode identity.

```sh
JMCL_ZIG_SOURCE=/path/to/pinned/core \
cargo test --locked --manifest-path rust/Cargo.toml \
  --test content_registry_enums -- --include-ignored --nocapture
```

The ignored source test compiles a pure registry adapter with Zig 0.16.0. It
invokes only `registry::parse`, never load/save, mutation RPCs or adoption.
Local Linux results: all three new tests passed, including 99 exact enum cases;
previous 212 registry-source and 189 typed-array cases plus the pure numeric
grammar regression remained green. Focused Clippy, formatting and diff checks
passed. Independent source review found no concrete additional defect. No
transport, archive, installation or runtime operation is involved. Native CI
is separate from local Linux results.

## Ordered-parser follow-up

The enum-only audit retained 23 malformed-string diagnostic/order differences.
The subsequent [private ordered registry parser](content-registry-ordered.md)
closes those named cases while preserving this enum grammar and public serde
contracts. Neither stage uses wildcard discrepancy allowances or global lexical
normalization; exhaustive malformed-input equivalence is not claimed.

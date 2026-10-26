# Typed content-provider numeric metadata

Pinned Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e` parses
Modrinth and CurseForge response documents with typed `parseFromSlice` schemas
(`src/content/modrinth.zig`, `src/content/curseforge.zig`). These fields retain
binary128 typed-number coercion; they are not dynamic RPC binary64 values.

The Rust content parser previously converted these numbers to `serde_json::Value`
first, losing the lexical distinction between `"7.0000000000000001"` and `7`.
The former must be rejected as `InvalidNumber`, not silently used as artifact
size 7. Borrowed raw lexemes now use the existing typed-number implementation,
with u64 for sizes, u32 for identifiers/pagination and u8 for enum-like fields.
Unknown-field policy, validation order after parsing, source limits, and dynamic
RPC number parsing are unchanged. This is a Rust fidelity repair, not a new
exception to the original contract.

`provider_numeric_lexemes_match_pinned_typed_grammar` covers 204 exact pure
parser comparisons across six fields, including raw/quoted precision boundaries,
integer widths, signed zero, exponent/underscore/hex grammar and malformed
values. The existing provider corpus remains enabled. These are synthetic JSON
fixtures, with no HTTP requests, credentials or artifact downloads.

`invalid_typed_metadata_prevents_download_and_preserves_registry` runs the actual
version-switch service with a synthetic metadata provider and an artifact
transport that panics if called. Invalid raw/quoted fractional sizes must return
`InvalidNumber` before artifact access and leave registry bytes/identity and
existing read-only shared-file bytes/identity unchanged. It does not test live
provider behavior, Java acquisition, or archive parsing.

Run with the pinned adapter built as documented in the Rust workflow:

```sh
JMCL_CONTENT_METADATA_ORACLE=/path/to/content-metadata-oracle \
  cargo test --locked --test content_remote -- --include-ignored --nocapture
```

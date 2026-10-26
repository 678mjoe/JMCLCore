# Synthetic authentication request byte-array decoding

The original protocol decodes authentication request Params through dynamic
`std.json.Value` into byte-string fields. Rust's inline request closure accepted
only JSON strings. A behavior-preserving extraction made that actual closure a
private, wired `parse_rpc_params` helper; its original field/error order and
Secret ownership were reviewed before testing.

The pure baseline contained 140 public-dummy buffers across five decode modes
and four source schemas. Seventy cases differed: legacy accepted empty arrays,
integer/integral-float bytes, raw binary64-rounded fractions, quoted integer
spellings, and valid multi-byte UTF-8 arrays. The Rust helper rejected them.
This is decoder acceptance evidence, **not successful authentication**: empty
values may still be rejected by subsequent validation, which these tests never
invoke.

## Narrow fix and ownership

Only private request string decoding changes. It reuses dynamic u8 numeric
coercion and requires checked UTF-8 before creating the existing `Secret` type.
The temporary `Zeroizing<Vec<u8>>` is allocated from the complete borrowed
element count **before** writing decoded bytes, so pushes cannot reallocate and
strand decoded bytes in superseded allocations. Numeric or UTF-8 errors drop
that wiping buffer; successful borrowed UTF-8 is copied once into a
`Zeroizing<String>` and transferred to `Secret`.

Client ID, secondary field, then begin-only optional locale retain their original
order. Missing/null locale remains absent; non-begin methods ignore it. Shared
response `text`/`optional` helpers, auth JSON decoding, later semantic validation,
token processing, transport construction, and request execution are unchanged.
No generic JSON tree is introduced into production secret decoding.

Opaque non-UTF8 arrays remain unsupported by the existing String/Secret
representation. `[255]` is explicitly rejected, while `[195,191]` decodes as
valid UTF-8. This is not full arbitrary-byte authentication parameter support;
future widening requires review of downstream form/header/string consumers.

## Pure tests

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  auth::request_param_tests::pure_auth_request_arrays_are_decoded_before_semantics \
  -- --exact
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  auth::request_param_tests::pure_auth_request_utf8_and_result_shape_controls \
  -- --exact
JMCL_ZIG_SOURCE=/path/to/pinned/core \
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  auth::request_param_tests::pure_auth_request_decode_matches_pinned_schemas \
  -- --exact --ignored --nocapture
```

The 305-case matrix verifies accepted decoded values exactly and compares
rejection acceptance separately: Rust's erased errors are not claimed equal to
Zig's detailed parser error names. Strict result-shape negative controls prevent
arbitrary oracle objects from being counted as rejection. Cases cover
raw-versus-quoted precision, integer grammar, fractional/overflow/wrong-type
bytes, decoded-prefix failures, null/default/missing fields, ignored fields,
UTF-8, and field ordering. Prefix/UTF-8 failure controls exercise early exits;
they do not claim to inspect allocator memory after wiping.

The pinned Zig adapter uses the real public Config/MicrosoftAccess types and
verbatim private Poll/Refresh declarations. Their dynamic parse options are
source-checked; source text checks allow CRLF checkout conversion. Config
validation methods and all authentication operations remain uncalled. The
checkout must be clean at `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, with Zig
0.16.0. Owned adapter inputs preserve bytes, mtime, readonly state and Unix
file identity.

All compared values are explicitly public dummy fixtures, never credentials.
No handler, transport object, request/login/refresh operation, network, token
store, account, Java or archive operation is invoked. Local Linux evidence and
source ownership review do not substitute for native CI or real service/UI
acceptance.

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

Private request decoding now retains byte strings in preallocated wiping buffers
through complete typed decoding and the existing protocol empty-field guards.
Only then does preparation check UTF-8 and convert to the existing `Secret`.
See [preflight rejection ordering](auth-preflight-order.md) for that follow-up.
No opaque bytes enter forms, headers, URLs, or transport-facing strings.

The temporary `Zeroizing<Vec<u8>>` is allocated from the complete borrowed
array element count **before** writing decoded bytes. String inputs copy from
the existing wiping JSON string decoder into a once-allocated wiping buffer.
All present buffers are checked before any `Secret` conversion. Numeric,
empty-guard, and UTF-8 failures drop the wiping buffers. Prefix controls are
error-path checks, not a proof of allocator memory erasure.

Client ID, secondary field, then begin-only optional locale retain their original
order. Missing/null locale remains absent; non-begin methods ignore it. Shared
response helpers, auth JSON decoding, later semantic validators, token processing
and transport bodies remain unchanged. No generic JSON tree is introduced into
production secret decoding. Private decode accepts `[255]` as bytes; wired
preparation rejects it at the source-compatible semantic error boundary, unless
an earlier typed error or protocol empty guard wins. `[195,191]` remains valid
UTF-8. This does not widen the public `Secret` representation.

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

The 335-case matrix (including the original 305 and 30 opaque-byte controls) verifies accepted decoded values exactly and compares
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

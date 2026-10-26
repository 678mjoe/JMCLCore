# Ordered CurseForge download-URL metadata parsing

This change concerns the **JSON metadata parser only**, not HTTP requests or
URI construction. Pinned `src/content/curseforge.zig::parseDownloadUrl` parses a
complete `Envelope(?[]const u8)` and then validates the optional URL. Its 2 MiB
input bound precedes parsing.

The former two-pass RawValue-envelope parser could return a later pagination
error before an earlier wrong `data` type, misclassify malformed strings, or
accept an invalid unknown string. An exact test of the actual private helper
found 31 differences in 68 cases. Thirty were ordinary parser fidelity gaps;
one was the already accepted opaque-URL limitation, not a bug to silently fix.

The helper now uses the existing ordered envelope and optional String reader,
then calls the unchanged `validate_url`. Missing required `data` remains
distinct from explicit null. Typed u8 array elements, pagination, duplicate
fields and skipped unknown strings are processed in source order. Full typed
structure and trailing-input checks precede URL semantic validation.

## Explicit compatibility deviation

[COMPAT-URL-001](url-compatibility-decision.md) remains in force. The test has one
exact named opaque-byte fixture, `https://a/` followed by byte FF: pinned source
returns that byte array; Rust returns `UnexpectedToken`. Both exact results are
asserted separately and the case is **not counted as source-equal parity**.
There is no wildcard mismatch allowance, lossy conversion, percent-encoding
adapter or additional accepted operational URL representation.

## Pure evidence

`url_ordered_tests` calls only the actual private metadata helper and a pinned
pure `parseDownloadUrl` fixture. The corpus contains 236 source-equal cases plus
the single explicit deviation. It covers missing/null/wrong data types, numeric
byte arrays, pagination order, malformed known/unknown Unicode, duplicate and
trailing syntax, deterministic mutations/truncations, deep unknown arrays,
exact/over-2-MiB documents and exact/over-2048-byte URL validation. Input bytes
and modification times are preserved.

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge::url_ordered_tests::malformed_unknown_url_metadata_is_not_accepted \
  -- --exact
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge::url_ordered_tests::ordered_url_metadata_matches_source_with_named_deviation \
  -- --exact --ignored --nocapture
```

The source matrix requires `JMCL_ZIG_SOURCE` at
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` and Zig 0.16.0. No client, resolver,
request send, provider access, artifact, runtime or archive operation is invoked.
The separate pure URL-construction contract remains the evidence for backend
normalization; none of these tests constitutes network acceptance.

## Scope and security

RawValue `Envelope` was removed after its last caller migrated. The now-unused
private generic Modrinth serde fallback parser was also removed following a
source/test consumer search; numeric and byte/string helpers remain unchanged.
No shared auth/dynamic RPC parser changes are included.

HTTPS checks, URL length and forbidden-character validation, host/SNI handling,
redirect policy and credential-origin restrictions remain unchanged. The fix
does not add a transport layer or change the user's accepted URL decision.

Local Linux results: both new tests passed (236 strict + 1 named deviation),
along with Project 247, Page 459, File 615, earlier CF 275/128/224/640,
Modrinth 1,410/781 and all 32 pure URL-construction pairs. Clippy (library and
tests), formatting and diff checks passed. Independent source review identified
stale envelope field accesses during drafting; these were corrected before the
passing run. Native execution remains a separate CI gate.

# Completed CurseForge single-record lexical acceptance

Private project parsing and public file parsing now validate the **full original
JSON document** only after both the envelope and the inner typed schema succeed,
and before semantic validation. This rejects invalid skipped outer strings and
unpaired surrogate escapes that Zig rejects, without eager generic checks.

`parse_file` now separates complete Detail deserialization from the existing
semantic validation/conversion. Page callers keep their old per-item
parse/validate sequence via `parse_file_data`; neither that loop nor generic
Envelope/RawValue or Modrinth parsing changes in this commit. Filename, URL,
transport, acquisition and numeric grammar remain unchanged.

## Evidence

Pinned oracle source `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, `parseMod` and
`parseFile`, using the existing pure CF test adapter. Baseline unit test accepted
an invalid outer FF string and was red. Final local evidence:

- 2 tests pass, including **128 exact pure oracle cases**: outer/inner unknown
  strings, before/after placement, invalid UTF-8 or surrogate escapes, valid
  UTF-8/pairs/escaped backslashes, large ignored numbers, semantic-invalid IDs
  and earlier typed-invalid ID controls. Input JSON bytes/mtime are preserved.
- Prior CF byte metadata/selection (3 tests, 275 cases), Modrinth lexical
  acceptance (100), cosmetic bytes (129), typed byte arrays (189), Modrinth
  metadata (84), CF schema (115) and typed numbers (204) remain green.
- Format, all-target Clippy and diff checks pass. No network, Java, archives,
  installs, workloads or real user data are touched.

```sh
cargo test --manifest-path rust/Cargo.toml --lib lexical_acceptance_tests -- --include-ignored
```

Configure `JMCL_ZIG_SOURCE` and Zig 0.16.0. The ignored test builds the unchanged
source's pure parser adapter in an owned temporary directory.

## Deliberately remaining limits

- Literal invalid UTF-8 inside `data` can fail the RawValue envelope stage before
  inner typed parsing; its existing `UnexpectedToken` versus source `SyntaxError`
  diagnostic is not fixed here.
- An earlier malformed ignored string followed by a typed error still follows
  the existing Rust parser error path; no speculative global offset ordering is
  used. Earlier typed errors before later bad strings remain unchanged.
- Page parsing still validates each item before typing later items. Source types
  the complete page first; page-order/acceptance needs a separately evidenced
  pure parsing refactor. No full-page acceptance claim is made here.
- Download-URL envelope acceptance and operational filename/URL bytes are outside
  this single-record scope.

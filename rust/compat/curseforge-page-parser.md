# Fully typed CurseForge page parsing

The private `parse_page` helper now deserializes a complete typed `PageEnvelope`
(`Vec<Detail>` plus the existing `Pagination`) before checking the 50-file limit
and applying per-file semantic validation. This follows pinned Zig
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/content/curseforge.zig:206` (`parseFilesPage`). A completed-document string
check runs only after typed success, not in generic Envelope or Modrinth parsing.

The baseline extracted helper reproduced the actual old resolver parsing order:
semantic-invalid first file plus missing-field second file returned
`InvalidCurseforgeId`, whereas the source returns `MissingField`. The same actual
helper test is green after the change; it is not a manual loop reproduction.

## Bounded evidence

- Three tests: the baseline regression, 224 exact pinned pure-parser cases, and
  a Metadata-only injected `Resolver.pages` test. The oracle corpus covers
  0/1/49/50/51 files; typed errors versus earlier semantic errors and length;
  data/pagination input order; all four pagination numeric/default fields;
  unknown malformed strings, valid controls, duplicates and an oversized body.
  Input bytes and modification times are checked unchanged.
- Injected local pages exercise empty/short/no-pagination termination, a 50+1
  page sequence, the per-page limit and the existing 512 aggregate guard. URLs,
  index increments, page size, metadata limit and fixture key are asserted. No
  real transport, provider request, resolver acquisition or artifact work occurs.
- The outer metadata get, aggregate guard and pagination stop/index rules remain
  unchanged. Public file/project parsers and generic Envelope remain unchanged.
- Prior CF byte metadata (275 cases), single-record lexical (128), Modrinth
  metadata (84), CF schema (115) and provider typed numbers (204) are green.
  Format, all-target Clippy and diff checks pass locally.

```sh
cargo test --manifest-path rust/Cargo.toml --lib page_parser_tests -- --include-ignored
```

Set `JMCL_ZIG_SOURCE` to the pinned clean source and put Zig 0.16.0 on PATH. The
ignored test compiles a pure parser adapter into an owned temporary directory.

This supersedes the page-order limitation in `curseforge-lexical-acceptance.md`.
It does not fix every malformed-string versus typed-error diagnostic/precedence
case: failed typed parsing still uses the existing Rust error path. Operational
filename/URL bytes, download-URL envelope parsing, native/live acceptance,
network, Java and archives are outside this slice.

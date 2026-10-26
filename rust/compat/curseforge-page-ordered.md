# Ordered CurseForge Page envelope parsing

Pinned `src/content/curseforge.zig::parseFilesPage` parses the complete typed
`Envelope([]File)` before applying the 50-file page limit and then validating
each file semantically. The 8 MiB input bound precedes all parsing.

The earlier serde page parser preserved count/semantic order but could displace
an earlier malformed string with a later typed or missing-field error. An
isolated test invoking the actual private `parse_page` helper found 28
differences in 57 source comparisons. Its output was an evidence collection,
not a parity pass. For example, an invalid unknown string in the first item
returned Rust `MissingField` instead of source `SyntaxError`.

The Page entry point now reuses the [ordered File schema](curseforge-file-ordered.md)
inside a complete ordered envelope. Nested files, hashes, dependencies and
pagination retain the same typed readers. The shared envelope returns parsed
data and optional pagination; File parsing still ignores the parsed pagination
value only after validating its schema. No per-item semantic validation or
50-item early cutoff occurs during token parsing.

Unknown subtrees remain iterative and input-bounded. The schema has fixed
nesting, exact raw typed numeric spans, duplicate-before-value checks and
unchanged String-versus-byte filename conversion. URL construction policy and
COMPAT-URL-001 are unchanged.

## Pure evidence

The new `page_ordered_tests` invokes the actual private Page helper and the
pinned pure `parseFilesPage` adapter. It retains the 57 original cases, adds
truncation and deterministic mutation controls, deep unknown containers,
and malformed final entries after 50 or 51 complete files, totaling 459 exact
source comparisons. Input bytes and
modification times are checked.

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge::page_ordered_tests::earlier_page_lexical_failure_precedes_missing_item_fields \
  -- --exact
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge::page_ordered_tests::ordered_page_matches_pinned_source \
  -- --exact --ignored --nocapture
```

The source comparison requires `JMCL_ZIG_SOURCE` at pinned commit
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` and Zig 0.16.0. These are parser tests;
no resolver loop, metadata fetch, artifact download, runtime or archive operation
is invoked. Local Linux results: the new regression and 459-case matrix passed; previous
File (615), Modrinth Project (1,410), Version (781), CF metadata (275), completed
acceptance (128), page (224), and typed envelope (640) corpora also passed. The
private filename/URL boundary test remained green. Independent source review
found no additional concrete issue. Native CI results remain separate.

## Unchanged boundaries

Resolver pagination, stop conditions and the aggregate 512-file limit remain
unchanged. Project and download-URL parsing are separate entry points and their
failed-typed diagnostics are not claimed fixed. No HTTP, authentication,
redirect, acquisition or shared dynamic JSON behavior changes are included.

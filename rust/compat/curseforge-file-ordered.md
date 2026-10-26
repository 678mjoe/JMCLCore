# Ordered CurseForge File envelope parsing

The File-only parser now consumes typed fields in source order, including nested
hashes, dependencies, byte-string arrays and pagination. It reuses the private
ordered cursor established for Modrinth, without changing its tokenizer. The
cursor visibility is limited to `crate::content`; no public parser API is added.

## Contract and boundary

Pinned source `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/content/curseforge.zig` defines `File`, `FileHash`, `FileDependency`,
`Pagination`, `Envelope` and `parseFile`. The 2 MiB response limit precedes
parsing. Required and duplicate fields are checked in encounter order; complete
typed envelope parsing and trailing-input checks precede unchanged file semantic
validation. Numeric fields retain the existing exact typed u8/u32/u64 grammar;
`releaseType` and `relationType` are integers, not JSON enum names.

The actual public `parse_file` previously returned `UnexpectedToken` for an
invalid unknown string before a later `id=true`, while the source returned
`SyntaxError`. A bounded 41-input investigation found 26 differences, including
later missing/duplicate/EOF errors displacing earlier string errors. This is not
fixed by a global serde error-category mapping: wrong integer containers must
still be rejected at their first token, without parsing malformed descendants.

The fixed schema has at most eleven fields in a 16-bit duplicate mask. Schema
nesting is finite; unknown subtrees use the existing iterative stack, bounded by
input size rather than an invented nesting limit. Tokenization advances through
input monotonically; there is no recursive unknown-subtree descent or repeated
whole-document parsing.

Public String filenames and private byte-preserving filenames retain their
separate conversion boundary. URLs retain the existing String boundary and
[COMPAT-URL-001](url-compatibility-decision.md); no HTTP or URI policy changes.

## Evidence

`content_cf_file_ordered` tests the actual File helper against the pinned pure
metadata adapter. Its corpus includes the 41 original cases, known/unknown
malformed strings, typed numeric bounds and spelling, nested containers,
pagination ordering, duplicate/missing fields, deterministic truncation and
mutation controls, deep unknown arrays, and exact/over-2-MiB input limits.
The expanded matrix contains 615 exact source comparisons. Input bytes and
modification times are preserved. A private pure test separately preserves the
String/byte filename distinction and unchanged opaque URL rejection boundary.

```sh
JMCL_ZIG_SOURCE=/path/to/pinned/core \
JMCL_CONTENT_METADATA_ORACLE=/path/to/pure/metadata-adapter \
cargo test --locked --manifest-path rust/Cargo.toml \
  --test content_cf_file_ordered -- --include-ignored --nocapture
```

Only parser functions and owned fixture files are exercised. No resolver,
transport, provider access, installation, archive or Java operation is needed.
Local Linux checks passed the 615-case File matrix and private boundary test,
plus the existing CF metadata (275), completed-parse acceptance (128), Page
(224), typed envelope (640), Modrinth Project (1,410), and Version (781) pure
corpora. Formatting and focused Clippy passed. Independent source review found
no additional concrete schema or indexing defect. Native CI remains separate
evidence from these local results.

## Deliberately unchanged

CurseForge Project, Page and download-URL entry points retain their prior parser;
their failed-typed lexical diagnostics are not claimed fixed. Page-wide typing
before the 50-record limit and semantic validation remains unchanged. Resolver
pagination and the 512-file aggregate limit are outside this change, as are
acquisition, credentials, redirects and HTTP behavior. Modrinth Project and
Version schema behavior is unchanged by cursor visibility adjustments.

A subsequent [Page-only increment](curseforge-page-ordered.md) reuses the File
schema in an ordered Page envelope; the File evidence and limits above remain
applicable. Project and download-URL parsing remain separate.

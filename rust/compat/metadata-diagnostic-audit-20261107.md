# Version and CurseForge diagnostic status

## Finding

The current candidate already contains the ordered Modrinth Version and all four
CurseForge metadata parsers. The earlier blanket statement that
"Version/CurseForge malformed-JSON diagnostic debt" remains open is stale for
the concrete cases documented by those increments. This audit found no new
reproducible failure in those existing corpora. It does not establish exhaustive
malformed-input equivalence or accept unspecified differences.

No production code, oracle source, expected output or test case is changed by
this reconciliation. In particular, no global error mapping, eager whole-input
validation or byte-preserving URL transport is introduced.

## Current error paths

- `content::remote::Resolver::{project,version,resolve}` passes fetched metadata
  to the real Modrinth parsing helpers. `modrinth::parse_version_as` and
  `parse_versions_as` call `version_ordered::{one,many}` before semantic validation.
  Their nested File, Hashes and Dependency schemas consume fields in source order.
  [Version evidence](modrinth-version-ordered.md).
- CurseForge `Resolver::resolve` calls `parse_project`, `parse_file_as` and
  `download_url`; `Resolver::pages` calls `parse_page_as`. These use
  `project_ordered::parse` and `file_ordered::{parse,page,url}`, respectively.
  File/Page/Project/download-URL all type complete ordered envelopes before
  semantics. The old serde RawValue envelope is no longer on these paths.
  [File](curseforge-file-ordered.md), [Page](curseforge-page-ordered.md),
  [Project](curseforge-project-ordered.md), [download-URL](curseforge-url-ordered.md).
- The unchanged pinned source functions are
  `src/content/modrinth.zig::{parseVersion,parseVersions}` and
  `src/content/curseforge.zig::{parseMod,parseFile,parseFilesPage,parseDownloadUrl}`.
  They use typed `std.json.parseFromSlice` with unknown fields ignored, followed
  by their provider validation. The source's normal service error paths call
  these parsers directly; the pure fixtures call the same functions.

The known regression shapes are invalid earlier unknown strings followed by a
later typed/missing-field failure, malformed known strings, complete-envelope
ordering, and Page-wide typing before count/semantic checks. Their existing
tests assert exact source error names. A wrong container at a typed boundary
still wins before malformed descendants, so blanket lexical prevalidation is
not a compatible shortcut.

## Reverification

Baseline: clean independent worktree at
`009b046ac0a4916c0fa5b073212cc7404db2e51a`, tree
`5e61790fde937ab927b2b79d1a59d3323f2dbaa3`. Oracle checkout:
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`, with no source modifications.
Toolchains verified locally: Rust/Cargo 1.99.1 and Zig 0.16.0 on Linux x86_64.
The pure metadata adapter was freshly built from the checked-in adapter and
that pinned source, not assumed from an older binary.

All 25 selected tests passed. Their existing matrices contain **5,015 exact
source-equal comparisons**, plus one separately asserted COMPAT-URL-001 case:

- Modrinth Project 1,410; Version/versions array 781.
- CurseForge File 615; Page 459; Project 247; download-URL 236 source-equal
  plus one named deviation.
- Earlier CurseForge byte metadata/selection 275, completed-parse acceptance
  128, complete-page grammar/order 224, typed-envelope ordering 640.

The existing finite corpus was rerun without modifying expectations. Because
production code is unchanged, there is no new red-to-green repair claim; the
red/green evidence for the original fixes remains in the linked increment docs.

Reproduce with a local pinned source, Zig 0.16.0 on `PATH`, and
`JMCL_CONTENT_METADATA_ORACLE` pointing to a freshly built pure
`rust/compat/content_metadata_oracle.zig` adapter:

```sh
cargo test --offline --locked --manifest-path rust/Cargo.toml \
  --test content_project_ordered --test content_version_ordered \
  --test content_cf_file_ordered -- --include-ignored --nocapture
cargo test --offline --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge:: -- --include-ignored --nocapture
```

These execute parser functions, owned temporary fixtures and the existing
injected-metadata pagination test. They do not use a real client, provider,
account, installation, archive or Java runtime. Input bytes and modification
times are checked by the paired corpora. Native Windows/macOS execution and live
provider behavior are outside this local evidence.

## Separate accepted URL boundary

[COMPAT-URL-001](url-compatibility-decision.md) is unchanged. The download-URL
corpus's one named opaque-byte case asserts both exact results separately and
is not source-equal parity. Existing URL normalization/representation choices
cannot excuse a new JSON diagnostic failure. A newly demonstrated difference
still requires its own source evidence and an explicit decision or repair.

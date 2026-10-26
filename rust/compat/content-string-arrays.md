# Typed content byte-array strings: bounded UTF-8 slice

Baseline `c0bda37fb66366123f3159fd0d9f99a00bd000b9`; unchanged Zig
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` remains the oracle.

The original typed `[]const u8` JSON decoder accepts ordinary strings or arrays
of typed u8 numbers. Array elements retain the same quoted/integral-float and
binary128 grammar as other typed fields. Rust's provider string visitor already
accepted integer byte arrays, but plain serde u8 rejected `"65"` and `65.0`, and
reported the wrong errors for fractional and out-of-range elements. It now uses
the existing raw-lexeme typed-width-8 parser for each element. No dynamic RPC
number coercion, URL handling, network transport or string validation changes.

Persisted content registry `file`, `sha1`, `source.project_id`, `version_id` and
`slug` previously rejected even valid ASCII byte arrays accepted by Zig. These
fields now use the same UTF-8 string/optional-string decoder; normal output
strings and disk schema are unchanged. Numeric decoding failures retain exact
`InvalidNumber`/`InvalidCharacter`/`Overflow` error names.

The new corpus also exposed an existing semantic-error collapse. Zig checks
project ID, version ID and optional slug in that order. Missing required IDs
produce `InvalidContentSource`; present malformed IDs retain
`InvalidModrinthVersionId` or `InvalidCurseforgeId`; malformed slugs retain
`InvalidModrinthProject`. Rust now preserves these names and precedence. Plain
string controls and equivalent byte arrays cover the correction. Accepted
source identities and their validation rules have not changed.

## Evidence and boundaries

- Baseline local new tests fail on ordinary registry ASCII arrays and typed
  provider byte errors. Separate pinned pure-parser observations confirm the
  original source-error-name mismatch also occurs for plain strings.
- Five focused tests pass, including **189 exact pure-parser oracle cases**:
  three provider surfaces, five registry fields, ordinary/quoted/float numeric
  grammar, bounds, malformed element types, ordered source validation and
  multibyte UTF-8 controls. Input bytes, mtime and read-only status are unchanged;
  Unix file identity is checked as well.
- The existing pure metadata, CurseForge schema and provider numeric corpora
  were rerun separately, without running their neighboring installation tests.
- Formatting and all-target Clippy pass. This is local Linux evidence, not a
  native four-platform acceptance claim; exact-head CI remains required.

The registry fixture imports the unchanged pinned `content_registry.parse`.
Its test verifies the original source revision and clean tracked `src` tree,
then compiles only the small pure-parser adapter. The provider adapter uses the
existing `JMCL_CONTENT_METADATA_ORACLE` contract. No Core RPC/CLI request, Java
program, archive/installer operation, provider HTTP request, adoption, mutable
world or game/server operation is performed by these focused tests.

**Non-UTF-8 byte strings remain a known, actionable representation gap.** A
separate regression verifies no lossy conversion or Latin-1 reinterpretation
has been introduced; that rejection assertion is explicitly **not** oracle
compatibility. This commit does not claim arbitrary byte-path takeover,
adoption compatibility or full provider compatibility. Those require preserving
bytes through metadata/registry types and capability-relative native paths;
they are not silently dropped or marked policy-blocked by this limited fix.

Run the bounded suite (pinned `zig` on PATH):

```sh
JMCL_ZIG_SOURCE=/path/to/pinned-zig-source \
JMCL_CONTENT_METADATA_ORACLE=/path/to/content-metadata-oracle \
  cargo test --locked --test content_string_arrays -- --include-ignored --nocapture
```

The existing native workflow exports these two variables and includes ignored
oracle tests. No new workflow or permissive compatibility exception is needed.

# Ordered Modrinth Version and versions-array parsing

A private nested-schema parser now consumes Version, VersionFile, Hashes and
Dependency in encounter order using the previously tested Project cursor. Public
record types and semantic validators are unchanged. The generic serde parser,
authentication JSON code, dynamic RPC numeric conversion and CurseForge parsing
are not migrated.

## Contract preserved

- The 4 MiB single-version and 16 MiB versions-array limits precede parsing.
- Each complete typed object detects duplicate fields before the duplicate value,
  checks required fields before returning, and validates unknown strings in order.
- Numeric fields keep raw spelling and use the existing binary128 unsigned helper.
  Wrong scalar/container tokens are rejected at the same typed boundary as Zig;
  invalid punctuation at value positions remains a lexical error.
- Dependency enums accept the source's named or integer-formatted values. Float
  spelling is not enum coercion. Negative zero is rejected because the source's
  unsigned enum-tag `parseInt` rejects a negative sign, unlike typed u64 coercion.
- The entire versions array is typed before checking its 512-item limit or any
  version's semantic constraints. The existing semantic order is retained.
- String and byte-preserving `FilenameValue` callers remain distinct. String
  filename rejection occurs at its original typed boundary. Opaque URL bytes
  still cannot become a String URL; COMPAT-URL-001 is unchanged. No URL transport
  construction, normalization or security policy changes occur here.

Unknown nesting uses the existing iterative stack, bounded by input length, not
recursive calls or a new depth limit. Known schema composition has fixed depth;
its arrays are processed iteratively. Cursor offsets move forward, scalar lexemes
are borrowed, and duplicate-field bitsets cover fixed private field lists. No
unsafe code, network, resolver, installer, executable or archive operation is
introduced.

## Evidence

The pre-fix actual public parser failed two new tests: an earlier invalid unknown
string lost to a later typed error, and source-compatible numeric dependency enum
values were rejected. The separately linked baseline library was built from
`b88a1d4`; relevant Modrinth sources matched the integration base.

The new pure source corpus compares 781 cases with pinned Zig Core `5a4310d`:
known/unknown lexical failures, nested schema errors, duplicates, missing fields,
wrong containers, enum/u64 grammar, bounded deterministic mutations/truncations,
unknown nesting at 160/512 levels, exact/over response sizes, semantic lengths and
512/513-item error ordering. Input bytes and timestamps remain unchanged. Two
additional private unit tests assert String/byte filename ordering and the
unchanged opaque-URL boundary. The Project 1,410-case corpus remains a separate
regression gate.

```sh
JMCL_ZIG_SOURCE=<pinned-source> JMCL_CONTENT_METADATA_ORACLE=<pure-adapter> \
  cargo test --locked --test content_version_ordered -- --include-ignored --nocapture
```

This closes the demonstrated Modrinth failed-typed ordering cases within the
finite corpus, not every possible malformed-input behavior. CurseForge/shared
parser diagnostics and live provider acceptance remain separate. Native CI for
this change is required independently of local pure Linux evidence.

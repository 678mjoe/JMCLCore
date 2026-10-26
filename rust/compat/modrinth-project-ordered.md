# Ordered Modrinth Project parsing

`parse_project` uses a private ordered token cursor. The subsequent
[nested Version parser](modrinth-version-ordered.md) reuses only narrow cursor
methods; CurseForge parsing, generic `parse<T>`, authentication JSON helpers and
dynamic RPC number conversion remain unchanged. Existing public Project semantic validation
still runs after complete typed parsing, in its original order.

The previous serde typed path consumed entire `RawValue` numeric elements before
checking their types and validated skipped strings only after successful parsing.
Consequently it could accept an earlier malformed unknown string until a later
schema error, or report EOF instead of an earlier invalid UTF-8 byte. Merely
mapping serde syntax errors or comparing offsets is not equivalent: a typed u8
must reject an opening container before examining its malformed children.

The private parser preserves raw numeric spans and calls the existing binary128
`typed_raw` helper. Slice fields peek token kinds before decoding wrong scalar
contents. It validates UTF-8, escape and surrogate sequences incrementally;
completed strings reuse the unchanged `auth::json::Json::string` decoder. Known
field duplicates are checked before the duplicate colon/value. Missing required
fields precede trailing-document validation. Unknown fields retain their original
ignore policy, including unconverted `1e400` numbers, but their lexical content is
checked in encounter order.

## Resource and security boundary

The existing 1 MiB Project response limit is checked first. Unknown objects and
arrays use an explicit iterative state stack, not Rust recursion or a new depth
limit. The cursor advances monotonically: tokenization and ignored-value walking
are linear in input bytes, with stack memory proportional to nesting depth and
therefore bounded by the input size. Known strings/byte arrays allocate only their
decoded content; existing typed numeric conversion remains unchanged. There is no
unsafe code, network, provider resolver, executable, archive, or storage operation
in this parser.

## Evidence and limits

The new `content_project_ordered` target passes 1,410 exact cases comparing actual `parse_project` output
against the pinned pure Modrinth adapter from Zig Core `5a4310d`. It includes
normal metadata, malformed UTF-8/escapes, typed byte grammar, duplicate/missing
field order, deterministic single-byte mutations and truncations, unknown
nesting beyond 128 (including 512 mixed levels), and exact/over-limit inputs.
Input bytes and timestamps are preserved. Existing 84 metadata, 204 typed-number,
100 completed-parse lexical, 129 metadata-byte and constrained-byte fixtures also
remain green; the focused parser target and library pass Clippy. The original parser regression is
checked against a separately identified pre-fix library with identical Modrinth
source, not a copied helper.

```sh
JMCL_ZIG_SOURCE=<pinned-source> JMCL_CONTENT_METADATA_ORACLE=<pure-adapter> \
  cargo test --locked --test content_project_ordered -- --include-ignored --nocapture
```

This is a bounded Project parser corpus, not an exhaustive malformed-input proof
or a fix for CurseForge/shared JSON diagnostics. Version evidence is documented
separately. Native platform execution
is a separate CI gate; no live service acceptance is inferred.

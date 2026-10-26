# Ordered CurseForge Project envelope

The Project parser now uses the same private ordered envelope machinery as
[File](curseforge-file-ordered.md) and [Page](curseforge-page-ordered.md), with
its own five-field schema. Pinned source is
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/content/curseforge.zig::{Mod,Envelope,parseMod,validateMod}`.

The CurseForge Project bound remains **2 MiB**, not Modrinth Project's 1 MiB.
Complete typed envelope parsing, including pagination and unknown fields,
precedes semantic validation. Raw numeric spans retain typed u32 grammar.
Unknown subtrees use the existing iterative cursor; no new nesting policy or
recursive scanner is introduced. Duplicate and required fields retain source
ordering and `isAvailable` still defaults to true.

## Evidence and slug boundary

The actual private helper failed 44 of 68 initial pinned comparisons. Earlier
malformed strings lost their `SyntaxError` precedence to later typed, missing,
duplicate or EOF failures. Raw byte-array slugs also returned `UnexpectedToken`
instead of `InvalidCurseforgeSlug`, or the earlier `InvalidCurseforgeId` when
both fields were semantically invalid.

A private parsed Project retains slug bytes until the complete envelope has
been typed. Validation still checks ID before slug; slug remains restricted to
1–128 ASCII letters, digits, hyphen, underscore or dot. Only then is it converted
to the existing operational String. This preserves rejection order; it does not
accept opaque operational identifiers or alter resolver/registry APIs. Display
name bytes retain their existing byte-preserving representation.

`project_ordered_tests` exercises the actual helper with the initial 68 cases,
additional u32 spelling/bounds, slug-length and semantic-order controls,
deterministic truncations/mutations, deep unknown arrays and exact/over-2-MiB
inputs, totaling 247 exact source comparisons. A separate pure assertion checks ID, ASCII slug, opaque display name,
class ID and the availability default. Fixture bytes and modification times
are preserved.

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge::project_ordered_tests::earlier_project_lexical_error_precedes_later_type \
  -- --exact
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::curseforge::project_ordered_tests::ordered_project_matches_pinned_source \
  -- --exact --ignored --nocapture
```

The source matrix needs `JMCL_ZIG_SOURCE` pinned as above and Zig 0.16.0. All
executed tests are pure parser calls; no resolver, metadata request, artifact,
installation, Java or archive operation is required. Local Linux checks passed all three new tests, plus Page (459),
File (615), Modrinth Project (1,410), Version (781), and previous CF
275/128/224/640-case corpora. Clippy (library and tests), formatting and diff
checks passed; independent source review found no concrete additional defect.
Native CI is separate.

The now-unused serde `TypedEnvelope` and completed-document validation helper
are removed because File, Page and Project are already ordered. The raw
Envelope and **download-URL parser remain unchanged**, with their diagnostic
debt tracked separately. URL construction decision COMPAT-URL-001, transport,
credentials, redirects, shared JSON/dynamic RPC semantics and acquisition are
outside this change.

The subsequent [download-URL metadata parser](curseforge-url-ordered.md) closes
its ordinary ordered-JSON gaps while retaining COMPAT-URL-001. This does not
change URI transport behavior.

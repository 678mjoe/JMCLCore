# Modrinth constrained byte fields and semantic rejection

Pure project/version/versions parsing now uses private wire structs for fields
whose original byte-string schema has ASCII/enum/hex semantic constraints.
Public `Project`, `Version`, `File`, `Hashes`, `Dependency` and their String-based
operational API are unchanged. Filename and URL remain the existing String
boundary; this does not implement raw path or transport support.

## Source order and conversion safety

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/content/modrinth.zig`, `parseProject`, `validateVersion`, `parseVersion` and
`parseVersions` establish:

- Project id/slug: ASCII alphanumeric plus `-_.`; project type: one of four
  fixed ASCII tags; loaders: ASCII alphanumeric plus `-`.
- Version/dependency IDs: ASCII alphanumeric; version type: three fixed ASCII
  tags; game versions: ASCII alphanumeric plus `.-_+`; loaders as above.
- SHA1: exactly 40 ASCII hex bytes. Actual integrity rules are not weakened.

All these constraints reject non-ASCII, including valid Unicode strings. The
private `MetadataBytes` wire fields preserve bytes until source-ordered semantic
validation. Only then does checked `String::from_utf8` conversion run; no lossy
conversion, fallback, placeholder, or unchecked decoding is used. Cosmetic byte
metadata remains unchanged. Dependency optional/null/default/type behavior is
retained. Entire versions lists are typed and lexically checked before length
and per-version semantic checks, preserving later structural error precedence.
Within each file, unchanged filename and URL validation precede SHA1 and size.

## Evidence

Baseline actual public helper: non-UTF8 id array returned `UnexpectedToken`
instead of source `InvalidModrinthVersionId`. A 240-case discovery corpus found
39 mismatches (20 semantic rejections and 19 missing-field precedence cases).
Plain UTF8, ASCII-array, valid Unicode rejection and plain typed-error controls
already matched; these controls remain in the regression matrix.

The new pure test uses all three actual public parsers and compares complete
successful output or exact error to the pinned oracle. It adds cross-version
structural precedence, duplicate fields, nested dependency enum errors and
optional IDs, and checks fixture bytes/mtime remain unchanged.

Two new tests pass with 252 exact cases. Prior cosmetic bytes (129), typed
arrays (189), MR lexical acceptance (100), MR metadata (84), CF schema (115),
provider numbers (204), CF metadata bytes (275), CF lexical (128), CF pages
(224) and CF envelope ordering (640) remain green. Formatting, all-target
Clippy and diff checks pass locally.

The previous cosmetic-byte test's operational assertions intentionally now
expect the source semantic errors for constrained fields, while raw filename
and URL still expect the existing `UnexpectedToken`. No rejected field becomes
an accepted operational byte path or transport string.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_constrained_bytes -- --include-ignored
```

Set `JMCL_CONTENT_METADATA_ORACLE` to the pinned pure metadata adapter. No live
requests, downloads, registry changes, installs, Java, archives or real user data
are involved. Known malformed-string lexer diagnostics and operational filename/
URL acceptance remain separate limitations.

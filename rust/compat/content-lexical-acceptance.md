# Completed Modrinth typed-parse lexical acceptance

Successful typed Modrinth project/version/version-array parsing now additionally
validates the source's UTF-8 and JSON Unicode escape pairing before semantic
provider validation. This closes an acceptance gap: serde skips unknown strings
without necessarily checking invalid raw UTF-8 or unpaired surrogate escapes,
whereas Zig's scanner rejects them with `SyntaxError`.

The check reuses the existing UTF-8 validation and `auth::json::Json::parse`
structural/Unicode escape validator. It does not materialize unknown fields,
parse their numeric values or introduce a JSON scanner. Unknown `1e400` numbers
remain accepted; valid surrogate pairs, UTF-8 text and escaped `\\ud800` text
retain their prior behavior.

**Order and scope are deliberate:** checks run only after the entire typed
Modrinth schema succeeds, and before semantic ID/content validation. Existing
schema/type/number/duplicate errors return unchanged. The generic `parse<T>`
function and CurseForge Envelope/RawValue parsing are untouched. A check after
an envelope parse would not imply its nested typed data was complete and could
incorrectly reorder errors.

## Evidence

Pinned source `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, Modrinth parse functions
and the unchanged pure metadata adapter. Baseline accepted an unknown FF string;
new unit test was red. Final evidence:

- 2 new tests pass, including **100 exact paired pure parser cases**. Invalid
  unknown values/keys/nested strings, raw FF, surrogate escapes, valid controls,
  and invalid semantic IDs before/after unknown lexical failures are covered.
- Earlier type and duplicate errors, and invalid nested containers passed to
  typed u8 array visitors, preserve their source error rather than being replaced
  by a later lexical diagnostic.
- Existing 129 cosmetic byte, 189 typed byte-array, 84 Modrinth, 115 CurseForge and
  204 typed numeric cases remain green. Input fixture bytes/mtime are unchanged.
- Formatting, all-target Clippy and diff checks pass. No filesystem mutation
  operations beyond owned test JSON fixture writes, network, Java, archives,
  acquisition or launched workloads occur.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_lexical_acceptance -- --include-ignored
```

Set `JMCL_ZIG_SOURCE` and `JMCL_CONTENT_METADATA_ORACLE` to the pinned source and
pure adapter.

## Explicit remaining diagnostics

Project and Version parsing now have separate [Project](modrinth-project-ordered.md)
and [nested Version](modrinth-version-ordered.md) ordered parsers/corpora. The
remaining text records the historical successful-parse-only boundary that
motivated those implementations; it is not a claim that the same demonstrated
Modrinth cases remain unfixed. Other shared/provider parser diagnostics still
need their own evidence and scope.

The original successful-parse-only change did not fix all malformed JSON
diagnostics/order. For example, invalid raw UTF-8 in a known title was `UnexpectedToken` instead of source
`SyntaxError`; an invalid unknown string followed by missing required fields
encountered serde's missing-field error first. These required the separate
ordered parsing solutions, not blanket prevalidation.

Investigated and rejected shortcut: mapping all serde Syntax errors or comparing
its final error offset to a string's invalid byte offset. For
`title:[{"unknown":"<FF>"}]`, Zig rejects the object as an unexpected u8 token
before examining its contents; the RawValue-based serde visitor consumes the
container and reports a later Unicode Syntax error. A naive offset/category rule
would break currently correct behavior. Earlier `id:true` and duplicate-field
controls likewise demonstrate why global eager lexical checking is unsound.

# Typed single-record CurseForge envelopes

`parse_file` and private `parse_project` use a complete typed envelope so the
input order of data and pagination controls typed errors, as in pinned Zig
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` `parseFile`/`parseMod`.

Previously the outer RawValue envelope deferred all nested data typing until
pagination had been parsed. Actual public helper evidence found five mismatches
in 32 pure cases: for example `data.id=true` before `pagination.index=1.5`
returned `InvalidNumber` instead of source `UnexpectedToken`; a missing file
field similarly lost its `MissingField` precedence. Reverse-order and semantic
error controls already matched.

The fix only adds `TypedEnvelope<T>` for these two callers. Existing raw Envelope
is retained for download-URL parsing; PageEnvelope and acquisition remain
unchanged. Complete typing still precedes the existing full-document lexical
check and semantic validation. No blanket error normalization is introduced.

## Evidence

The new actual-helper regression calls both private project and public file
parsers, with plain typed errors and missing fields before later pagination
errors. The 640-case pinned pure oracle matrix covers project/file, valid/type/missing/
semantic data, all four pagination fields, typed-number lexical controls and
both field orders. Tests preserve input bytes and modification times.

Two new tests pass; prior page (224), CF metadata bytes (275), single-record
lexical (128), Modrinth metadata (84), CF schema (115) and typed-number (204)
cases remain green. Formatting, all-target Clippy and diff checks pass locally.

```sh
cargo test --manifest-path rust/Cargo.toml --lib typed_envelope_tests -- --include-ignored
```

The ignored test requires the pinned clean `JMCL_ZIG_SOURCE` and Zig 0.16.0.
No requests, provider downloads, filename/URL pipeline changes, Java, archives,
installs or real user data are involved. Malformed known-string diagnostics and
earlier malformed unknown strings followed by typed errors remain separate
limitations; this is not a complete lexer parity claim.

The later [File-only ordered parser](curseforge-file-ordered.md) replaces the
File serde envelope to preserve failed-typed lexical ordering. Project retains
the typed envelope described here; Page and download-URL parsers are unchanged.

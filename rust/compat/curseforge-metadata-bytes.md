# CurseForge byte metadata and source-faithful selection

The existing typed-u8 `MetadataBytes` representation now covers four bounded CF
surfaces: project display name, hash values for ignored non-SHA1 algorithms,
file date and game-version tags. This is not a cosmetic-only change: date is an
operational selection tie-breaker and game tags determine compatibility.

## Consumers and invariants

- `curseforge::Project.name` is parsed but unused by resolution. It now accepts
  the source's byte-string representation without lossy conversion.
- `Hash.value` remains subject to the original typed-u8 parser. For algorithm 1,
  byte length must be exactly 40 and every byte ASCII hexadecimal before the
  validated value is converted to String. A later invalid SHA1 still fails even
  after an earlier valid SHA1; missing SHA1 remains an error. Other algorithms'
  values are ignored by integrity enforcement, exactly as before.
- `Detail/File.date` compares raw byte slices lexicographically, including
  prefixes and exact ties (first item wins). Release rank, availability and
  filename-extension filters are unchanged.
- `Detail/File.games` validates byte lengths, matches the requested game exactly,
  and matches recognized loader tags using ASCII-only case folding. Unicode
  normalization, case folding and replacement characters are not introduced.

Filename, URL, numeric identifiers, slug, request arguments, transport, CAS,
registry and envelope ordering are untouched. The new byte accessor does not
reinterpret bytes as a filesystem path. No new RPC or live provider calls exist.

## Source and evidence

Oracle source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, CF `parseMod`,
`parseFile`, `parseFilesPage`, `fileSupports`, `selectFile` and
`selectFileWithExtension`. A new pure adapter imports those unchanged functions.
It is compiled in an owned temporary directory after checking the pin and clean
source tree; it only reads synthetic JSON and prints parser/selection results.

Baseline unit test was red (`UnexpectedToken` for a raw project name). Final:

- 3 new unit tests pass, including **275 exact pure oracle cases** for typed byte
  arrays, invalid element types/ranges, FF/WTF-8/UTF-8 metadata, game matching,
  loader ASCII matching, date prefix/ties, availability/rank/extension selection,
  actual SHA1 validation and ignored algorithms.
- Prior 129 cosmetic-byte, 189 typed-byte, 100 Modrinth lexical-acceptance,
  84 Modrinth, 115 CF schema and 204 typed numeric oracle cases remain green.
- Format, all-target Clippy and diff checks pass. Input fixture bytes/mtime are
  preserved by parser/selection calls.

```sh
cargo test --manifest-path rust/Cargo.toml --lib byte_metadata_tests -- --include-ignored --nocapture
```

Set `JMCL_ZIG_SOURCE` and provide Zig 0.16.0. The test adapter is
`tests/fixtures/curseforge_metadata_bytes.zig`; private-schema tests live in
`tests/support/curseforge_metadata_bytes.rs`, included only under `cfg(test)`.
No network, archive parsing, Java, installation, real user data or workloads are
executed. Native/remote publication is not claimed by this local evidence.

Remaining separately scoped work includes operational filename/URL bytes and
CF malformed unknown-string acceptance/diagnostic ordering. No generic
Envelope/RawValue lexical precheck is added here.

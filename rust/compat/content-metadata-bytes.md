# Supplemental Modrinth metadata preserves arbitrary bytes

This bounded stage follows the typed UTF-8 byte-array repair (`1ab3d33`). It
changes only these four original `[]const u8` / optional byte-string fields:

- `Project.title`
- `Version.version_number`
- `Dependency.file_name`
- `Hashes.sha512`

Source review found no operational consumers of these four content fields in
`rust/src`: they are retained metadata. The optional **content** `sha512` field
is not used for integrity enforcement; the real SHA-1 verification path is
unchanged. The unrelated `modpack::schema::Hashes.sha512` type and its validation
are untouched. `Dependency.file_name` is not the install destination filename.

The new `MetadataBytes` representation decodes strings or arrays with the exact
typed-u8 element grammar already established. Serialization emits valid UTF-8
as the original JSON string and arbitrary other bytes as integer arrays, just
like pinned Zig. No replacement characters, Latin-1 reinterpretation, dropped
bytes, path conversion or new RPC/API schema is introduced.

Operational fields—IDs, URL, destination filename, actual SHA-1, game versions,
loaders and all registry/CAS paths—remain on their separate UTF-8 representation.
Their unresolved byte-path/validation gaps are not declared complete here. The
shared string decoder still explicitly rejects non-UTF-8 for those fields; a
regression checks these boundaries. The earlier registry raw-byte rejection
control is retained, while its former title control now uses an operational
filename because arbitrary title bytes are intentionally supported.

## Bounded proof

Pinned oracle: unchanged Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, through
`JMCL_CONTENT_METADATA_ORACLE`. Before this change, a pure fixture with title
`[65,255,66]` fails with Rust `UnexpectedToken` while Zig retains the byte array.
The same discrepancy was independently observed for the other three fields.

Three focused tests now pass, including **129 exact parser/serialization
comparisons** across the four fields and both single-version / versions-list
parsers: normal strings, ASCII and multibyte UTF-8 arrays, arbitrary invalid
UTF-8 and surrogate-byte sequences, embedded zero, quoted/integral-float bytes,
fractional/overflow/type errors, optional null/missing, required missing and
parse-before-semantic-error ordering. Input bytes, mtime, read-only status and
Unix identity remain unchanged. Operational-field controls verify no broadened
raw path or lossy conversion.

The preceding 189-case typed-array/registry corpus and the existing pure
84-case Modrinth, 115-case CurseForge and 204-case provider numeric corpora are
rerun separately. Formatting and all-target Clippy are checked. These are local
Linux results; exact native CI is still required.

These four fields are not directly returned by current content RPC/CLI methods.
This evidence establishes **metadata acceptance and representation**, not direct
RPC wire coverage or real provider/service acceptance. No HTTP, installation,
adoption, archive, Java, account, game/server or mutation operation is executed.

A distinct pre-existing lexer-diagnostic difference remains: a raw invalid UTF-8
byte inside a quoted JSON literal produces Zig `SyntaxError` versus Rust
`UnexpectedToken`. That malformed-JSON issue is not normalized or counted as
fixed by byte-array support and should receive its own bounded parser audit.

```sh
JMCL_CONTENT_METADATA_ORACLE=/path/to/content-metadata-oracle \
  cargo test --locked --test content_metadata_bytes -- --include-ignored --nocapture
```

The existing native workflow already provides this adapter and includes ignored
oracle tests. No new provider request, compatibility allowlist or native-path
policy change is necessary for this stage.

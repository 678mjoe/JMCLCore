# Opaque provider metadata fields

Local continuation from `8df9b61`; published numeric snapshot remains unchanged.

The modpack provider adapter used String for CurseForge project `name` and file
`fileDate`. The fixed Zig schemas use `[]const u8`; both accept strings and typed
byte arrays, including empty arrays and non-UTF-8 bytes. Neither field is used to
construct the returned Project/ProviderFile or to perform semantic validation.

Both fields now use the existing MetadataBytes decoder. This preserves complete
byte-array semantics without changing returned types, paths, downloads, secrets,
validation ordering or transport behavior. Other pack display-name/path fields
remain separate outstanding work; this is not a general modpack byte-string fix.

The new Rust pure regression failed before the change on an empty name array.
It calls only project/file response decoding on synthetic in-memory JSON. Cases
cover ordinary strings, empty arrays, invalid UTF-8/NUL bytes, numeric strings,
integral floating tokens, byte overflow, fractional tokens and wrong JSON types.
The paired Zig standard-library test exercises the exact `[]const u8` field type;
it is not an end-to-end Core or network test.

Final checks pass: both pure provider tests (including 16 new metadata cases),
the pinned Zig type test, formatting, all-target Clippy, and 22 static guard tests
in each Python mode. Independent review confirmed the fields are unused by
output/validation and that missing-field requirements remain unchanged.

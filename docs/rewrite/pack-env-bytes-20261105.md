# Pack client/server environment byte values

Continuation from 25b94a7. Env.client/server now use optional MetadataBytes.
The same three accepted environment values are compared as complete bytes,
at the original end of File.validate. Client inclusion compares bytes against
unsupported. No lossy conversion, paths, downloads or operational routing change.

The first pure test failed before the production change on an array spelling
of required. Two new tests cover required/optional/unsupported, optional nulls,
empty/opaque/NUL/unknown values, byte overflow and fractional tokens, original
file-size error precedence, and full Mrpack.parse through client selection.

The pinned original mrpack module pure fixture parses an in-memory index and
checks shipsToClient plus original environment/file-size error precedence.
No archive, installation, Java, filesystem or network operations run.

Final local checks: four schema tests, Zig pure fixture, formatting, all-target
Clippy, and 28 static guards in each Python mode. No remote writes or CI changes.
Cross-platform native execution and actual pack installation remain unverified.

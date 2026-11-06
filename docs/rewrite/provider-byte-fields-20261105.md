# Provider hash and game-tag byte fields

Continuation from 4993dd2. The modpack CurseForge adapter now decodes Hash.value
and each gameVersions element with the existing MetadataBytes typed decoder.
The pinned Zig implementation uses byte slices for both fields.

Game tags retain their original count and byte-length checks. Non-SHA1 hashes
retain arbitrary typed bytes; every SHA1 is still validated, including later
entries after the first valid SHA1. A validated SHA1 is necessarily ASCII and
converts losslessly to the existing String output. No URL, path, transport or
install interfaces changed.

The first regression failed before the production change with UnexpectedToken.
The preserved red patch is separate from the implementation. Four pure provider
tests pass, including two new tests covering opaque/NUL bytes, byte-array SHA1,
ignored hashes, missing SHA1, subsequent invalid SHA1, byte overflow/fractions,
wrong types, tag length 64/65 and count 128/129.

provider-byte-fields-oracle.zig imports the pinned actual CurseForge module and
runs only the named pure test via --test-filter 'pure provider byte fields'.
It calls parseFile and sha1Of in memory, verifies opaque fields and original
error ordering for invalid SHA1 and empty game tags. No transport, archive,
Java, filesystem fixture or installation is executed.

Local checks: formatting, all-target Clippy and 28 static guards in each Python
mode. No remote write or CI allowlist expansion. Native Windows/macOS compilation
and operational flows remain untested for this local snapshot.

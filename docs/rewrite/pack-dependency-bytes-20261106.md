# Pack dependency byte strings

Continuation from 7f1d708. Mrpack minecraft and four optional loader dependencies
retain typed bytes. Loader selection preserves quilt-first rejection,
Fabric/Forge/NeoForge order, conflict-before-later-version validation, and the
original ASCII version checks. Selection strings are created only after those
checks. Minecraft validation remains after loader selection; identity uses a
fallible lossless UTF-8 conversion rather than introducing a panic.

Both new tests failed before the change. They cover successful byte-array
minecraft/loader versions through identity and Selection, quilt plus invalid
game version, invalid first loader, conflict with invalid second loader,
invalid/empty minecraft and malformed numeric byte values.
The pinned original Mrpack parse/loaderSelection fixture confirms the same
successes and exact error priorities without any archive or install work.

Final local checks: eight schema tests, Zig pure fixture, formatting, all-target
Clippy and 28 static guards in each Python mode. No remote write, network,
credentials, Java, installation, or CI scope expansion. This is not validation
of manually constructed invalid packs or of an operational installation.

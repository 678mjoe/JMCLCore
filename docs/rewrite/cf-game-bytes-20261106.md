# CurseForge game-version byte strings

Continuation from 7284ead. Minecraft.version now retains typed bytes, validates
at the same original point, and is converted losslessly by identity with a
fallible UTF-8 conversion. The obsolete string-wrapper validator is removed.
Loader ID behavior is deliberately unchanged; its arbitrary-byte suffixes
need a complete separate downstream representation change.

The regression failed before the patch. It covers ASCII array success through
identity, empty/opaque/65-byte versions, invalid game-version priority over
invalid loader syntax and excessive loader count, and malformed typed bytes.
The pinned original CurseForge pure parser confirms success and error priority.

Final checks: ten schema tests, Zig pure fixture, formatting, all-target Clippy,
and 28 static guards in each Python mode. No archive, installation, Java,
network execution, remote publication or bounded CI scope changes.

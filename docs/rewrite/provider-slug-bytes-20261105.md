# Provider project slug typed bytes

Based on 9a0d006, project slug now uses MetadataBytes before semantic validation.
The same ID-first validation, nonempty/128-byte bound and ASCII alphanumeric or
hyphen/underscore/dot whitelist are preserved. Only after successful validation
is the slug converted losslessly to the existing String output.

The regression first failed on an ASCII integer-array slug with UnexpectedToken.
It now covers string, integer-array, numeric-string/integral-float bytes, invalid
opaque/NUL/slash/empty/129-byte slugs and ID error precedence. The fixed Zig
parseMod module counterpart confirms accepted ASCII arrays and matching errors.
Only pure parsing is executed; no transport, credentials, filesystem, archive or
install workflows. The red test patch is preserved separately.

Local checks: five provider tests, actual Zig pure module test, formatting,
all-target Clippy, 28 static guards in each Python mode. Remote branches and
bounded CI allowlist are unchanged; other native platforms are not revalidated.

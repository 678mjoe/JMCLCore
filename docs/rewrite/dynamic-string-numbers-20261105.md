# Dynamic server numeric strings retain typed precision

Local continuation from `79c596c`, not included in the published `190be14` snapshot.

Zig 0.16.0 `std.json.parseFromValue(u64, ...)` distinguishes numeric Values from
string Values. Numeric floats already have binary64 precision; strings call
`sliceToInt`, preserving parseInt grammar or binary128 float conversion. The Rust
server number helper incorrectly routed numeric strings through Rust decimal
parsing or binary64 fallback.

Examples:
- `"1_0"` must yield 10, not rejection.
- `"0x1.8p+1"` must yield 3, not rejection.
- `"3.0000000000000001"` must fail as a fractional value, not round to 3.
- `"18446744073709551615.0"` must yield the exact u64 maximum.
- The unquoted numeric token `3.0000000000000001` still yields 3 after the existing
  dynamic JSON Value rounding. This numeric behavior is intentionally unchanged.

`u64_value` now delegates string Values to the already-tested typed
`json_number::unsigned_text` helper without falling back after failure. Number
Values keep the original integer/binary64 branches and existing bounds. The old
test asserting binary64 coercion for strings was incorrect and is corrected.

The regression was red before the fix on `"1_0"`. The paired Zig test in
`rust/compat/dynamic-string-numbers-oracle.zig` calls only standard-library
`parseFromValue` using constant in-memory inputs; no Core RPC, authentication,
network, server, filesystem fixture, installation or archive work is invoked.

This change affects server parameter numeric strings and byte-array numeric
components which share the helper. Persisted typed records and other RPC helpers
are not modified. Pure regression evidence is not full operational parity.

Validation completed: four server-number tests, two shared JSON-number tests,
the pinned Zig pure coercion test, formatting, all-target Clippy, and 22 static
coverage guard tests in normal and optimized Python modes. The static report
still claims zero executed Core requests and disallows replacement.

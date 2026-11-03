# Pure unsigned CLI number grammar correction

Implementation checkpoint: `bb3b5d96641a034b2d4f949db07d20f435c1af74`.
Pinned Core original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
Standard parser reference: Zig 0.16.0 `std.fmt.parseInt`.

## Defects

Store CLI age parsing removed underscores before parsing and mishandled a second
sign after minus. This accepted `+_1`, `-_0`, `-+0` and `-_+0`, rejected by Zig.
A pure regression against the extracted original Rust expression failed on
`+_1` before the fix. No store was opened and no collection was run.

Version list limit parsing also removed all underscores without checking their
positions, accepting `_1`, `1_` and `+_1`. This was established by source review;
no malformed request was sent to a network provider.

Both callers now use a shared pure decimal unsigned parser. It permits one sign,
nonempty ASCII digits with internal underscores, negative zero only and values
up to u64 max. Existing immediate validation and version limit 1..1000 remain.
This range is equivalent to the original usize parser even on 32-bit targets.

## Evidence and boundaries

- 32 numeric cases pass in the Rust helper test and in an independent pure Zig
  standard-library reference test. Includes max/overflow, separators, signs,
  negative zero, exponent/fraction rejection and non-ASCII digits.
- 15 pure version parser cases cover valid values, malformed limits, range bounds
  and an invalid earlier duplicate that must not be rescued by a later value.
- Two Rust tests passed, none ignored. Offline build, all-target warning-denied
  Clippy, formatting and static audit tests (22 normal, 22 optimized) passed.
- Only Linux ran. These are pure parse/source results, not GC or provider end-to-end
  evidence. No Core store invocation, deletion, network, Java or archive operation
  was performed. No remote write or production replacement is authorized.

Pure reference command: `zig test rust/compat/store-cli-number.zig`.
Rust selection: `cargo test --offline --locked --lib cli_number_tests`.

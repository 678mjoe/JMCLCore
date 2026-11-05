# Pure server properties error precedence

Local continuation from `1c463c4`; not part of published `190be14` or its four-platform CI.

The pinned Zig parser at `5a4310debbcee26b3d2391bab2339f79cd6ec52e` decodes both fields before validating the key and then the value. Rust previously converted the value to UTF-8 before validating an otherwise UTF-8 key. Consequently `b"=\xff\n"` reported `InvalidServerPropertyValue` instead of `InvalidServerPropertyKey`.

The fix shares `validate_key` with `Document::set` and applies it after both escape decodes and key UTF-8 conversion, before value UTF-8 conversion. This preserves malformed value-escape precedence, duplicate ordering and mutation behavior.

Verification:
- New Rust test failed before the fix on the exact empty-key counterexample.
- Four pure Rust properties tests pass, including five key classes crossed with invalid UTF-8 values and malformed Unicode escapes, plus valid-key controls.
- The corresponding pinned Zig test passes using the actual oracle properties module. It imports no executable entry point and calls only in-memory `parse`.
- No server, socket, filesystem fixture, installation or Java archive operation is executed by these tests.
- Passing these tests establishes parser error classification only; higher-level handlers may collapse these errors. No full runtime equivalence or cross-platform claim is made for this change.

To run the exact Zig corpus with Zig 0.16.0 from the repository root, supply the pinned checkout path:

```
zig test --dep properties -Mroot=rust/compat/properties-precedence-oracle.zig -Mproperties=/path/to/pinned-oracle/src/server/properties.zig --test-filter 'pure precedence corpus'
```

Rust scoped check:

```
cargo test --locked --offline --manifest-path rust/Cargo.toml --lib servers::properties::
```

The static dispatch guard initially rejected the changed properties source digest.
Independent review confirmed unchanged RPC associations and operational paths.
Only that source digest and the reviewed commit were refreshed; blocked methods,
runtime evidence and replacement gates were not relaxed. Formatting and all-target
Clippy pass. The 22 guard tests pass in normal and optimized Python modes.

A follow-on deterministic differential corpus compared 32,910 in-memory parses
against the actual pinned Zig module with zero differences. It checks complete
ordered key/value byte sequences and error names, including NUL and invalid UTF-8.
The reproducible adapters and corpus are in `rust/compat/properties-pure-corpus/`.
The Rust adapter removes only the unused serde serialization derive. This is
additional finite parser evidence, not 32,910 Core CLI or operational tests.

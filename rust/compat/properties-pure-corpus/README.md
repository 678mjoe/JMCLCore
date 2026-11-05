# Bounded properties parser corpus

This harness compares only in-memory `properties::parse` results, including every
ordered key/value byte sequence or exact error name. It does not call server,
network, filesystem, account, installation or archive APIs.

The Rust adapter compiles the actual properties source with only its one unused
`serde::Serialize` derive removed; no parser code is copied or rewritten. Zig
imports the actual pinned source module. The binaries receive hexadecimal input
arguments, so raw NUL and invalid UTF-8 are preserved.

Use existing Rust 1.99.1 and Zig 0.16.0 installations. Create a new owned temporary
build directory and perform these steps from the repository root:

1. Verify the oracle checkout is `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
2. Copy `candidate.rs` and `oracle.zig` from this directory into the build directory.
3. Copy `rust/src/servers/properties.rs` there, removing only the exact substring
   `, serde::Serialize`. Check it occurs exactly once before removal.
4. Compile with `rustc --edition=2024 BUILD/candidate.rs -o BUILD/candidate`.
5. Compile with `zig build-exe --dep properties -Mroot=BUILD/oracle.zig
   -Mproperties=ORACLE/src/server/properties.zig -femit-bin=BUILD/oracle`.
6. Run `python rust/compat/properties-pure-corpus/compare.py BUILD`.

The deterministic 32,910-case corpus includes every 0–4 byte string from a
12-byte syntax/control/invalid-UTF-8 alphabet, 289 crossed Unicode/length cases,
and 10,000 seeded inputs. Results record binary and adapted-source SHA-256 values.
Every difference fails the script. This finite corpus is not an exhaustive proof,
a Core CLI test count, or a four-platform test claim.

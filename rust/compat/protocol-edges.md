# Numeric and framing compatibility evidence

Oracle: Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, Zig 0.16.0.
Initial Rust snapshot: `eed1130b43b7312a0f71a1dbec20a775e3038648`;
attested binary SHA256 `3a267c1c9141a318fe961026e627e7445b6599e17a5ba32666279f215d9ba10e`.
No real credentials, network requests, account modifications, or live instances.

## Why identical-looking numbers have different contracts

This is an observed implementation detail, not an attributed design intention.
The request envelope has a typed `protocol: u32`. Zig `std/json/static.zig`
`sliceToInt` (lines 766–774 in the pinned toolchain) parses integer spellings
using `std.fmt.parseInt` and other spellings using `parseFloat(f128)`. It checks
integrality before range. Numeric strings permit signs and internal underscores;
integer underscores may repeat, whereas floating underscores must lie singly
between digits. JSON number grammar itself does not permit underscores.

`Request.params`, however, first becomes `std.json.Value`.
`std/json/dynamic.zig::Value.parseFromNumberSlice` stores finite JSON floats as
binary64; oversized integer and non-finite float lexemes remain number strings.
`static.zig::innerParseFromValue` therefore uses binary64 for finite float params,
but binary128 for numeric strings. Consequently protocol `1.0000000000000001`
is invalid, while the same unquoted number used as `store.gc.min_age_seconds`
becomes 1. Its quoted form is invalid. Testing only ordinary integers misses this.

Rust now preserves raw spans at the envelope and GC coercion boundary. Shared
`json_number::typed_raw`, `dynamic_raw`, and `unsigned_text` make the distinction
explicit. Typed floats use pinned `rustc_apfloat = 0.2.3`, the pure-Rust LLVM
APFloat port maintained at <https://github.com/rust-lang/rustc_apfloat>.
`ieee::Quad` has IEEE binary128 semantics; parsing uses nearest-ties-to-even.
Source rounding, integrality-before-range checks, negative zero, and Zig lexical
rules are preserved separately; APFloat's permissive empty exponent syntax is
not inherited. This is not a decimal-integer approximation or f64 preconversion.
No platform C floating-point ABI or native quadmath library is introduced.

## Reproducible bounded evidence

- `protocol_edges.py`: exact exit/stdout/stderr comparison, including numeric
  strings/floats, precision, unsigned limits, duplicate/escaped keys, ignored
  fields, invalid surrogate pairs, missing/null/array params, and error precedence.
  The initial 122-case probe found 11 differences: eight f64 rounding differences
  and three numeric-string underscore differences. All 122 matched after repair.
  The checked-in corpus additionally covers dynamic-GC numbers and ignored-field
  validation with real implemented method names (141 cases total).
- `numeric_oracle.zig` and `generate_numeric_fixtures.py` generate the 342 checked-in
  `protocol-numbers.json` cases directly using the pinned Zig standard parser.
  Widths 8, 32, 64; decimal/hex strings; u64 limits; enormous positive/negative
  exponents; binary128 midpoint rounding; signed zero; error classes and priority.
  The Rust unit test consumes every case without skipping.
- Existing framing `check.py`: 51 exact cases matched; the existing three buffered
  oversized-line cases remain explicitly unresolved, not normalized into parity.
- Existing five injected authentication oracle suites passed with zero ignored
  tests after the shared numeric helper migration. Only synthetic transport data.

Example reproduction (use an isolated target directory):

```sh
zig build-exe rust/compat/numeric_oracle.zig -O ReleaseSafe -femit-bin=/tmp/numeric-oracle
python rust/compat/generate_numeric_fixtures.py --oracle /tmp/numeric-oracle --output /tmp/protocol-numbers.json
cargo test --lib json_number
python rust/compat/protocol_edges.py --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust
JMCL_AUTH_ORACLE=/path/to/injected/auth-oracle cargo test --test auth_oracle -- --include-ignored
```

## Approved source-defect correction: dynamic u64 upper-bound abort

Already observed once each, not repeatedly executed by this corpus:

```json
{"protocol":1,"id":"x","method":"store.gc","params":{"store_directory":"","min_age_seconds":18446744073709551615.0}}
{"protocol":1,"id":"x","method":"store.gc","params":{"store_directory":"","min_age_seconds":18446744073709551616.0}}
```

Pinned Zig exits by abort (`-6`), no stdout, stderr begins
`panic: integer part of floating point value out of bounds`.
The params are parsed before the empty directory can be rejected; no filesystem
operation occurs. `innerParseFromValue` compares against `floatFromInt(maxInt(u64))`,
which rounds to 2^64 in binary64, then checked-casts that same value to u64.
Rust safely returns the existing `INVALID_PARAMS` response rather than deliberately
reproducing the abort. This remains a documented source-defect exception approved under the user
source-defect correction principle. No Zig main/oracle change was made.
The quoted `"18446744073709551615.0"` is a valid typed binary128 u64 and must NOT
be rejected merely because the unquoted dynamic path has this source defect.

## Limits of this repair

The shared helper is applied to the envelope, typed authentication response
numbers, content install workers/retries, and GC params. Other typed schema
consumers that materialize `serde_json::Value` or use `visit_f64` before numeric
conversion still lose source precision; they require explicit per-caller migration
and source error-order tests. In particular this change alone does not establish
binary128 parity for instance/server manifests, metadata, or all provider schemas.
There is no broad allowlist, generic-output normalization, or full replacement
claim. Existing oversized-buffer, inherited-pipe, and other policy gates remain.

Final local checks for this patch: 141/141 raw protocol observations equal;
342/342 typed numeric golden observations equal; 152 ordinary library tests passed,
7 existing fixture-dependent library tests not run by that command; five explicitly
enabled authentication oracle tests passed, zero ignored. These counts are separate,
not an all-target/all-platform pass. The default `stable` toolchain lacked Clippy;
verification uses the installed `+1.99.1` toolchain instead.

### Proven remaining caller checklist (not hypothetical)

1. `instances.rs::schema_number`, used by the disk instance manifest:
   place `{"schema_version":1.0000000000000001,"id":"demo","name":"Demo","version_id":"1.0"}`
   in a fresh fixture `<root>/demo/instance.json`, then call `instance.get` with
   `{"directory":"<root>","id":"demo"}`. Zig emits `INSTANCE_LOAD_FAILED`;
   Rust still succeeds. Quoting that schema number produces the same mismatch.
2. `servers/manifest.rs` uses that same visitor. The identical fixture in
   `<root>/demo/server.json` and `server.get` produces Zig `SERVER_LOAD_FAILED`
   versus Rust success. This is independent of server process launching.
3. `metadata.rs::parse_json -> unsigned` loses the numeric span through Value.
   In otherwise-valid version metadata, set `downloads.client.size` to raw or
   quoted `1.0000000000000001`: pinned `metadata_oracle.zig metadata` returns
   `InvalidNumber`, whereas Rust `metadata::metadata` returns size 1. Setting it
   to quoted `18446744073709551615.0` succeeds with u64 MAX in Zig but returns
   `Json("Overflow")` in Rust.

The metadata fixture is:

```json
{"id":"1.0","type":"release","mainClass":"example.Main","downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1.0000000000000001,"url":"https://piston-data.mojang.com/client"}},"libraries":[]}
```

These tests only read synthetic local JSON; the URL is not contacted. Repair
acceptance is equality on these exact negative and positive cases plus existing
module oracle suites. Disk schema readers must be separated from
`servers/rpc.rs`'s `Params.grace_ms`, which currently shares `schema_number` but
is deserialized from a dynamic Value. Do not fix disk precision by silently
changing the RPC's finite-binary64 semantics. Metadata/provider consumers need
raw-lexeme retention local to typed-schema parsing, not a global serde feature
change that alters dynamic params.

`cargo +1.99.1 clippy --lib -- -D warnings` passed for this patch.


### Disk-schema follow-up closure

The proven instance/server manifest mismatches above are now repaired by a
separate typed `schema_number` raw-span deserializer. Server RPC `grace_ms` keeps
its own `dynamic_schema_number` visitor and therefore finite binary64 behavior.
The schema shape check uses RawValue, not Value, so an ignored `1e9999` field is
not prematurely rejected. Existing invalid-surrogate validation remains.
The 12 checked-in disk cases cover both raw/quoted rejected fractions, accepted
binary128-round-to-one strings, u32 overflow, and ignored huge exponents.
The complete raw corpus now has 153 cases. Metadata/provider adoption remains a
separate unresolved caller task; the metadata reproductions above still apply.

Local follow-up tests: 154 library tests passed (7 pre-existing fixture-dependent
library tests not run); eight explicitly enabled instance CLI/RPC oracle tests
and one server document oracle test passed, all zero ignored.

Follow-up verification also passed `cargo +1.99.1 clippy --lib -- -D warnings`,
the full 153-case raw edge corpus, and the existing 494-case server matrix
(its documented JSON/timestamp comparison rules are unchanged).

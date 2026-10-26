# Dynamic version request enum compatibility

Scope: `version.list`, `version.get`, `version.resolve`, and `install.plan`
parameter preparation only. No handler, Service, transport, metadata request,
or install planner is executed by these tests.

The private canonical enum view feeds the existing validation and consumers.
It borrows ordinary named values and copies only when a declared enum field
needs canonicalization; caller input is immutable. Only `source`, list `type`,
and plan `os`/`arch` are interpreted. Optional null/default behavior, unknown
fields, ordinary string validation, transport and execution bodies remain
unchanged. Dynamic integer tags and integer-shaped quoted tags are accepted;
float Values (including raw `-0`, `0.0`, and exponent forms) and CLI aliases
are not. Enum schema rejection precedes existing semantic guards.

## Evidence boundary

Pinned source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, Zig 0.16.0.
The adapter checks mirrored schema fields/defaults and dynamic parse flags
against that source. It compares actual private preparation/validation helper
acceptance and canonical values, **not full handler behavior or exact source
error labels**. A separate pure unit checks exact existing Rust error messages,
defaults, guard order and caller immutability.

Test-only baseline commit `b1aaeee` retains the original 106-case expected-red
proof: 43 differences (33 enum spellings, five UTF-8 byte arrays, five opaque
byte arrays). Baseline test ELF SHA-256:
`73ed1038b2b3a7d363c3de0d10c71ea6ada3b0e77459b894b4c99e6297fd2752`.
Raw results and source/artifact manifest are retained in
`version-params-proof-evidence/`, outside the repository.

The corrected gate retains **all 106 rows**. It asserts the exact ten remaining
byte-field discrepancies by method and case, separately from the 96 strict
matching enum/control rows. Those ten are unresolved compatibility work, NOT
approved deviations or green byte support. Operational IDs and loader values
still have String-only consumers. This slice does not claim complete request,
handler, install, or overall Rust parity.

Named tests:
- `version::params_contract_tests::version_parameter_enums_match_pinned_dynamic_schema`
  (ignored; requires pinned Zig/source; writes all rows with `JMCL_PARAMS_PROOF_OUTPUT`).
- `version::params_contract_tests::enum_schema_precedes_semantic_guards_without_mutating_input`
  (pure, no subprocess or filesystem operations).

Local Linux verification: both named tests executed exactly once and passed;
106 rows retained with 96 exact matches and the exact ten byte differences.
All 33 enum baseline differences closed. Test ELF SHA-256:
`477fae699f7aab975d495ccc9ca306e29a9ea30e627050311bb6d220aaafbd7f`.
No native Windows/macOS evidence is established by this local run.

Static `cargo clippy --all-targets -- -D warnings` also passed.

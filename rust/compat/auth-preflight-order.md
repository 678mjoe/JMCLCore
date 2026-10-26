# Authentication request rejection ordering (pure guard boundary)

The original protocol completes dynamic typed decoding, checks non-begin empty
client/secondary fields, then invokes input semantic validators. Every original
request validator rejects non-UTF8 input before request construction. Thus opaque
request bytes do not require a different HTTP/Secret representation; the
compatibility issue was their premature rejection by Rust's decoder.

A behavior-preserving extraction made the actual wired preparation and locale
predicate independently testable. Against pinned original source
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`, 226 public-dummy buffers exposed 62
phase/message differences: 46 generic semantic-error versus method-specific
parse-error differences and 16 cases where an empty counterpart should win.
This baseline executed original pure validators and verified source-derived
protocol guards, **not a protocol handler or authentication operation**.

## Private boundary

1. Decode all required request fields into preallocated `Zeroizing<Vec<u8>>`
   buffers, preserving dynamic numeric grammar, field order and locale defaults.
2. Apply the existing non-begin client/secondary byte-empty checks and exact
   method-specific messages.
3. Borrow-check every present buffer as UTF-8. Failures return `INVALID_PARAMS`
   with `Authentication parameters are invalid` through the existing handler
   error writer. A malformed later typed field still wins before this check.
4. Only after all checks succeed, copy into wiping strings owned by the existing
   `Secret` tuple. Drop/wipe every raw buffer on all paths.

The downstream client/opaque/locale validators remain in their original order
and place; valid UTF-8 semantic rejection is unchanged. Forms, headers, URLs,
response decoding, token handling and transport bodies are unchanged. No invalid
bytes cross into transport-facing strings. This does not claim matching internal
phase locations: final code/message or successful guard outcome is compared.

## Explicit pure test boundary

Run individually, never by a broad authentication module filter:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  auth::preflight_tests::pure_prepare_rejection_precedence -- --exact
JMCL_ZIG_SOURCE=/path/to/pinned/core \
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  auth::preflight_tests::pure_prepare_matches_pinned_guards \
  -- --exact --ignored --nocapture
```

The oracle uses a private owned copy of the pinned source, with only a pure
validator accessor appended. Normalized textual source hashes bind both original
authentication and protocol files (CRLF checkout differences are allowed).
Original protocol empty guards are reproduced from reviewed source; they are
not executed handlers. The fixture never constructs a transport or calls an
OAuth method. The original checkout is not modified.

The final corpus retains all 226 baseline cases plus 110 prefix/order controls.
It compares exact error code/message, or the positive pure-guard outcome. A
separate assertion checks successful Secret values exactly. The existing three
request-decoder tests now check raw byte values, not an obsolete standalone
UTF-8 decoder. Inputs are owned public dummy fixtures; bytes, modification time,
readonly state and Unix identity are checked unchanged. These are not a memory
wipe measurement, network test, login test, or full frontend/service acceptance.
Native CI remains a separate gate.

Local Linux validation passed both named preflight tests (336 exact outcomes)
and the three named decoder tests (335 acceptance/exact-value comparisons).
These are pure fixture results only; no authentication handler was executed.

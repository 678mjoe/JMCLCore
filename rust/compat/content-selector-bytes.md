# Dynamic RPC content file-selector byte arrays

Only `file` for existing local content remove/enable/disable RPCs now accepts the
source byte-string representation. Other request fields, CLI OS arguments and
String public Root APIs retain their previous behavior. Linux server content
uses the same byte-selector helper; its ordinary text CLI converts its existing
String file argument to bytes without changing the argument decoder.

`dynamic_file` decodes a JSON string or an array of raw numeric elements, using
`json_number::dynamic_raw(..., 8)`. This matches Zig's dynamic `Value` then
`parseFromValue([]const u8)` path, **not** the typed metadata byte visitor:

- JSON numeric floats use binary64 before the integral/range check.
- Quoted numeric strings retain typed numeric grammar/binary128 behavior.
- Out-of-range, fractional, boolean, object and nested-array elements fail
  parameter parsing. Successfully decoded unsafe paths fail selector validation.
- Empty/null selectors, project/file conflicts, and parse-error precedence
  retain the source's distinct diagnostics.

Client decoding preserves its existing last-key-wins Value behavior and all
other field decoding. Server decoding preserves its existing structural parser
and field behavior. The common byte path validator is only made visible to its
parent module; its rules do not change. No filesystem/CAS or global argument
changes occur in this commit.

## Evidence

Pinned unchanged Zig source `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/protocol/content.zig` / server content equivalent, and Zig 0.16
`lib/std/json/static.zig` dynamic integer/slice conversion. Original Rust returned
`INVALID_PARAMS` for valid ASCII file arrays where Zig returned
`CONTENT_NOT_FOUND`; the baseline new test was red.

Final Linux results:

- New `content_selector_bytes`: **558 exact paired RPC calls** covering 31
  grammar/selector-conflict cases × three local operations × four client/two
  Linux server families. Owned empty registries and directory counts are unchanged.
  Includes raw `109.00000000000000001` versus its quoted form, quoted/integral
  numbers, overflow, signed zero, underflow, invalid nested types and unsafe paths.
- Extended `content_mutate_bytes`: adds **486** exact paired raw-array selector
  mutation steps to the prior corpus, for **1467** total pairs. Physical UTF-8,
  WTF-8 and FF paths, enabled/disabled removal, missing files/parents, idempotence,
  untouched unrelated raw entries and shared readonly peer/CAS inode are checked.
- Prior 7 RPC + 4 CLI content oracle suites, 2 byte-adoption and 5 byte-listing
  tests pass. Format, all-target Clippy and diff checks pass.

Windows physical WTF-8 assertions are pending native CI, not inferred from Linux.
Physical-name capability exclusions remain limited to platform EILSEQ and are
reported explicitly. All executed operations concern owned disposable opaque
files; no archive parsing, downloads, Java, real data or workloads are involved.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_selector_bytes -- --ignored
cargo test --manifest-path rust/Cargo.toml --test content_mutate_bytes -- --include-ignored
```

Configure `JMCL_ZIG_SOURCE`/`JMCL_ZIG_ORACLE` to the pinned source and its binary.
Integration must review/repin whole-file dispatch-audit hashes after combining
source changes. Raw OS CLI file arguments remain the separate next boundary:
this change does not fix `main`'s `std::env::args()` UTF-8 requirement.

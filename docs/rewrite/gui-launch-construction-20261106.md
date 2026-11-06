# GUI launch construction, in-memory evidence

Baseline: `da3cc63ceb9c527358f888be627bbf52e6f2d35a`, tree
`5043cb562e1cc709267e2c779232462dcb7ae025`.
The pinned Zig oracle remains `5a4310debbcee26b3d2391bab2339f79cd6ec52e`;
neither that executable nor its launch adapter was run for this slice.

## Why this covers part of the GUI launch flow

The [GUI contract](../gui-handoff.md#launch) exposes `launch.plan` diagnostics
and launch events. Its production RPC implementation builds an install plan
and then a private launch plan, from which it obtains `redacted_argv` and
`sensitive_argv_indices`. The new `rust/tests/gui_launch_plan.rs` exercises
those same construction functions directly:

1. Normalize fictional metadata with `metadata::metadata` and, for legacy
   assets, `metadata::asset_index`.
2. Compute classpath and asset projections with `plan::build`.
3. Supply a manually constructed `Runtime`, borrowed fictional credentials,
   GUI-style options, and the same game/data directory to `launch::build`.
4. Check every private argv element, JVM/main/game boundary, public redacted
   element, and sensitive index against independently written expectations.
5. Send synthetic `Started` and already-redacted `Log` values through the
   real `EventWriter` into an in-memory writer and compare the exact JSONL
   bytes, including field order, source, Base64, merged sequence and newline.

This turns the ordinary launch test coverage from one smoke test into explicit
construction assertions without requiring a Java installation or the external
adapter. The existing smoke and two ignored adapter tests are unchanged.
No production source, CI allowlist, protected guard, executable entry point,
or GUI pin is changed. No production construction defect was found, so this
slice is test coverage, not a claimed bug fix or oracle-parity result.

## New coverage

- Modern vanilla: ordered/deduplicated library classpath, excluded library and
  JVM rules, default JVM settings, custom JVM values, logging placement, main
  class, ordinary authentication placeholders, resolution/demo flags and custom
  game values. Exact sensitive positions are `[2, 10, 26, 42]`.
- Legacy: generated native/classpath JVM options, Java 8 fallback when metadata
  has no Java requirement, actual planned virtual asset projections, asset-index
  precedence over `assets`, quoted/escaped/empty arguments, JSON properties and
  embedded token substitution. Exact sensitive positions are `[10, 24]`.
- Six token/session cases: token inside session, session inside token, equal
  secrets, absent session, explicitly empty session, and overlapping Unicode
  secrets. Metadata substitutions and repeated secret-bearing custom arguments
  are each redacted as one whole argv entry with no duplicate indices. Absent
  session falls back to the token; explicit empty session remains empty and its
  substitution slot is still marked sensitive.
- Custom JVM/game arguments remain literal, including placeholder-looking text,
  spaces and shell-looking punctuation. This proves construction, not OS spawn
  behavior or shell-injection resistance at execution time.
- Disabled default JVM settings do not leave stale sensitive indices. Linux,
  macOS and Windows target strings select the expected classpath delimiter.
  Path joins use the test host's separator; these are not native-platform runs.
- Partial versus complete resolution (including width zero), quick-play support
  and single-player options, and last-matching allow/disallow rules select the
  expected arguments. A placeholder in an excluded argument is not evaluated.
- Manual Runtime major/architecture mismatch and missing architecture return
  precise construction errors. `force_java` skips only those checked runtime
  gates; empty required authentication still fails. Unknown/malformed
  placeholders and malformed legacy quoting/escaping fail construction.
- Event output preserves the private argv, flushes each successful event, and
  stops writing after a synthetic flush failure. Started events expose only the
  expected redacted argv. Log fixtures include NUL/non-UTF-8 bytes, `newline`,
  `more`, and `eof`; the writer's source labels are checked independently of
  installation-source resolution.

## Independent boundary

This is **not** an end-to-end `launch.plan` RPC pass. That handler's normal
resolver enters Java resolution, so it is deliberately not called. Its request
decoder, result-envelope serializer, error mapping and runtime/source selection
are outside this evidence. `EventWriter` writes execution event envelopes,
not the `launch.plan` terminal result. The synthetic PID is never a live process.

All inputs and output sinks are in memory. Test bodies do not read or write
files, fetch metadata, install artifacts, extract archives, resolve/probe Java,
download runtimes, call `managed::resolve`, `executor::prepare`, or
`process::run`, or start a Core/Java process. Cargo's compilation artifacts and
captured test logs are development outputs, not launcher filesystem behavior.

`EventWriter` does not redact raw process logs. Its log inputs in these tests
are already redacted; streaming secret detection across reads and its overlap
handling remain untested here. Secret wiping is not audited by these tests.
Loader resolution, installation/preparation, actual natives/assets/classpath
existence, login, process lifetime, GUI rendering, real playability, native
Windows/macOS behavior and complete replacement readiness are not established.

## Reproduction and observed results

Use the already installed pinned Rust 1.99.1 toolchain and cached dependencies.
From `rust/`, run only these selected targets (do not add `--ignored` or
`--include-ignored`):

```sh
cargo test --offline --locked --test gui_launch_plan
cargo test --offline --locked --test launch_plan_differential native_plan_smoke -- --exact
cargo test --offline --locked --lib launch::rules::tests::
for test in vanilla_order_and_rewrite path_safety_and_sha1 rule_order_and_features dedup_and_conflict; do
  cargo test --offline --locked --lib "plan::tests::$test" -- --exact
done
cargo clippy --offline --locked --all-targets -- -D warnings
cargo fmt --check
git diff --check
```

Fresh Linux results on 2026-11-06:

- New GUI construction target: 8 passed, 0 ignored.
- Existing launch smoke: 1 passed; both external-adapter tests filtered out.
- Launch rules: 2 passed. Four exact install-plan tests: 4 passed.
- Locked/offline all-target Clippy with warnings denied: passed. This compiles
  other test targets but does not run them. Format and diff checks passed.
- An initial non-exact `plan::tests::` filter also matched three existing
  `java::runtime_plan::tests` cases. They passed and were source-reviewed as
  pure catalog/manifest/path validation, with no runtime resolution or I/O.
  The four intended install-plan tests were rerun by exact name. These three
  supplemental tests are not counted in the 15 focused tests above.

Captured logs are under `/workspace/shared/gui-launch-plan-20261106/`:
`green.log`, `regression.log`, `plan-exact.log`, and `clippy.log`.
`first-run.log` records an initial test-helper lifetime compilation error,
corrected before the successful run. It is not a production red/green defect
claim. No full test suite, operational test or external oracle adapter was run.

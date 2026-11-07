# Host-accurate owned probe fixture, 2026-11-07

The native Java-named test fixture previously reported `amd64` and 64 bits on
every host. It now reports its compiled `std::env::consts::ARCH` and
`usize::BITS`, with x86_64 spelled `amd64` for the Java property. The other
supported fixture names, aarch64/x86/arm, are recognized by the existing parser.
This changes only two test files, not production Java detection/selection.

## What changed and why

- `rust/tests/fixtures/java_probe.rs`: real architecture and pointer-width
  properties instead of unconditional amd64/64.
- `rust/tests/java_process.rs`: the exact native probe test checks returned
  executable path, normalized host architecture and bit width. The first
  managed-receipt result also asserts major 17, its fixture path and host arch.

The potential ARM64 problem was a false-positive test, not a demonstrated native
ARM failure. Production Rust managed selection filters receipt architecture and
then returns a successful probe without comparing its architecture again. The
pinned Zig implementation has the same behavior. That production contract is
unchanged; this increment does not claim a production architecture fix.

The fixture has three reviewed call sites: the two `java_process` tests and
`rust/compat/launch_rpc.py`. That script uses an explicit owned override and has
no fixed successful probe-architecture expectation. It was not run here. Other
intentional parser/selection architecture fixtures were not rewritten.

## Integrated verification

- Integrated local commit: `a924fe6c7572ab8b0fe0a24975c7860cdb966e03`.
- Integrated tested tree: `8005b88fa3c92a705c48bf2c23ca072eb575f2d4`.
- Independent original fix: `b616ec38d4c7ce7951862c12616bdd30afe6a4c5`,
  tree `8963bbaf7c0866465c9bb169bed4bc6bea5d52c3`, based on `13c7044c…`.
- Integration was on top of the retained modpack evidence; only the two test
  files changed. Production sources, dependency manifests/lock and CI remained
  byte-identical. Later retained evidence does not change the tested sources.

The clean integrated tree passed all of these with exit 0 on **Linux x86_64**,
using existing Rust/Cargo 1.99.1 and the offline dependency cache:

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
rustfmt --edition 2024 --check rust/tests/fixtures/java_probe.rs
cargo test --manifest-path rust/Cargo.toml --offline --locked --all-targets --no-run
cargo clippy --manifest-path rust/Cargo.toml --offline --locked --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml --offline --locked --test java_process native_probe_deadline_and_option_sanitization -- --exact --nocapture
```

The last command ran **1 passed / 0 failed / 0 ignored**, 1 filtered out.
`JMCL_TEST_JAVA_MODE` was removed from the initial environment; PATH selected the
existing rustc. Only a newly compiled owned executable is probed, including its
existing timeout/large-output modes. The hold mode creates an owned descendant
that sleeps two seconds; after a three-second natural-exit wait, no live argv
under the owned TMPDIR remained and the directory was empty. This is a
post-wait observation, not an assertion of immediate descendant reaping.

## Negative control and scope

The earlier isolated Linux worktree deliberately changed x86_64's fixture output
from amd64 to aarch64. The new architecture assertion rejected it with Cargo
exit 101 (`arm64` versus `x86_64`). Restoring the source made the same exact test
pass, followed by format/check/Clippy. This is a local lying-fixture negative
control; the old fixed-amd64 output already matched x86_64, so it is **not** an
ARM64 native red/green reproduction. Its separate logs are retained alongside
the fresh integrated logs in the [manifest](../../rust/compat/evidence/probe-fixture-arch-20261107/manifest.json).

The complete `managed_receipt_preference_and_download_error_policy` test was
compiled but not run: its later fallback branches discover system candidates.
Its new first-result assertions therefore are not a new managed runtime pass.
No actual JVM, system discovery, Java acquisition/install/runtime archive, game,
network action, Windows/macOS/ARM64/32-bit native run or broad suite ran in this
slice. No tool denial occurred; Rust replacement remains unapproved.

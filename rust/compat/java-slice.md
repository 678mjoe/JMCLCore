# Java rewrite slice

Baseline: Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

## Implemented surface

- `java.detect`, `java.runtime.list`, `java.runtime.remove`: `java::handle`.
- `java.select`: `java::select::handle`, returning the shared `version::Response`;
  source must precede result in the JSONL envelope.
- CLI equivalents via `java::cli`.
- `java::managed::resolve`: internal local/managed/forced policy engine, with an
  injected downloader. This is not itself a runtime installation implementation.
- `java::runtime_plan`: pure Mojang catalog/manifest and Azul package parsing.

`java.runtime.install` is **not implemented**. Do not dispatch it to these pure
planners, fabricate progress events, or mark runtime installation parity passed.
Pending: staged publication, executable permissions, symlink creation, verified
streaming downloads/retries, bounded archive extraction, original event ordering,
probe-before-receipt and failure cleanup. Runtime trees remain separate from the
instance hardlink/CAS mechanism, matching the baseline rather than inventing a
new on-disk migration.

## Compatibility details worth preserving

- `managed` is a preference, not a ban on local fallback. `local` bans download.
- An explicit absolute Java override bypasses major/architecture compatibility
  checking, but must still probe successfully and cannot accompany explicit paths.
- Installed receipts are filtered by their recorded major and platform before
  probing; the baseline does not then revalidate the reported JVM against those
  receipt fields. Tightening this requires a separately reviewed policy change.
- `java.select` uses the requested metadata source, not preferred-source fallback.
- Receipt reads permit a file symlink alias; removal unlinks descendants without
  following them. Root ancestor symlinks may resolve; the final root must be a
  directory rather than a symlink.
- Discovery preserves candidate order and per-runtime-home deduplication. Probes
  clear JVM option-injection variables and cap each pipe at 256 KiB with a 10-second
  total deadline. Descendant-held pipes cannot cause an unbounded reader join.
- CLI architecture aliases `amd64`/`aarch64` are accepted; RPC enum fields accept
  only canonical architecture names, as in Zig.

## Evidence

`compat/java.py --zig ... --rust ...` runs 59 exact raw CLI/RPC/disk checks with
preseeded metadata cache and synthetic executables. It makes no runtime download.

`cargo test --lib java::` covers parser/policy/storage edge cases. The native
`java_process` integration fixture compiles a real host executable (including PE
on Windows), tests bounded probes, installed-receipt preference and download
error mapping without downloading or installing a JVM.

`JMCL_ZIG_ORACLE=... cargo test --test java_receipts -- --ignored` proves a
Rust-written receipt is directly readable by Zig without migration.

Linux tests pass locally. Native Windows execution remains a CI gate. Before the
network metadata dependency was added, the Java slice cross-checked for Windows;
with reqwest/aws-lc, local cross-check requires the absent MinGW C compiler.
Do not report that as a Windows runtime pass. Mojang/Azul pure planners currently
have unit coverage, not full installed-runtime differential coverage.

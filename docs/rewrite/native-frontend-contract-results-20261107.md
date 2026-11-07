# Frozen native frontend contract results, 2026-11-07

## Result

**All four standard public native runner jobs passed** in the [final run](https://github.com/678mjoe/JMCLCore/actions/runs/42388086740). Each ran exactly two existing typed-wrapper scripts and six existing native Session tests. Every exact native invocation reported one passed, zero failed, zero ignored. The 13 pure guard regressions also passed normally and under Python `-O` on every final job.

This is finite typed-wrapper and `Session/LocalProcessTransport -> Core` acceptance. It is not Tauri IPC, a full Tauri crate build, GUI interaction, packaged sidecar discovery, or release/replacement approval. `replacement_allowed=false` and `full_compatibility=false`.

## Immutable subjects

- Core: `bdd6cb3e55d69cb9e0e02037d251635e289207a8`, tree `0e8e901bc587cf3c6d7dc1ba78821c4701785e20`.
- Frontend: `9c0fd81ce8109c1e35229560fee31308ca114ef5`, tree `48ea91981bdb7b375bca8e9bc47738b6c3dde4d1`.
- Executed harness: `d697c8fd12766979aed4c3190a3b19fc094b4f7d`, tree `b7d8af853e43ca401eb8252b078f9ec58b913561`.
- Rust `1.99.1`, Bun `1.4.4`; locked dependencies. Registry preparation occurred before offline/locked Core build and smoke execution. No real business service request is part of the selected slice.
- Checkout action `de0fac2e4500dabe0009e67214ff5f5447ce83dd`; setup-bun action `0c5077e51419868618aeaa5fe8019c62421857d6`.

## Final native results

### Ubuntu 24.04 x86_64 (`ubuntu-24.04`)
- [Successful native job](https://github.com/678mjoe/JMCLCore/actions/runs/42388086740/job/128449654102)
- Observed host: `Linux-7.0.0-1017-azure-x86_64-with-glibc2.39`; machine `x86_64`.
- Rust host: `x86_64-unknown-linux-gnu`.
- Core executable: `ELF64/x86_64`; 28352360 bytes; SHA-256 `07fcbf083222fc48ec5491c2a7f60749739cee3c30e22ba3f82ae7332cc8b935`.
- Session test executable: `ELF64/x86_64`; SHA-256 `44adb4d34518c2afe0b6d2e8defa1ba3f1963384a8ed98b938b3c8c9563ae9da`.
- Counts: 2 typed wrappers passed; 6 exact native selectors passed individually as 1/0/0.
- Report SHA-256: `49e8d7e322955b8033fb14132c7e61726f437a3a7259420dd570c96b7f3de11a`.

### macOS 15 ARM (`macos-15`)
- [Successful native job](https://github.com/678mjoe/JMCLCore/actions/runs/42388086740/job/128449654281)
- Observed host: `macOS-15.8.2-arm64-arm-64bit-Mach-O`; machine `arm64`.
- Rust host: `aarch64-apple-darwin`.
- Core executable: `Mach-O64/aarch64`; 26164864 bytes; SHA-256 `0b5cecbf35b60490e1ca84ebaf79b52e5689fd511c17806b2e0c7e041a0b9355`.
- Session test executable: `Mach-O64/aarch64`; SHA-256 `de19d890c7b29ca2a749302f2dcb09f8d04bce4347ea88787ba3a90627968505`.
- Counts: 2 typed wrappers passed; 6 exact native selectors passed individually as 1/0/0.
- Report SHA-256: `cb604159cd32eb4049bd94d6f2f859982063c928b267e8893c7426d54ff68141`.

### macOS 15 Intel (`macos-15-intel`)
- [Successful native job](https://github.com/678mjoe/JMCLCore/actions/runs/42388086740/job/128449654305)
- Observed host: `macOS-15.7.9-x86_64-i386-64bit-Mach-O`; machine `x86_64`.
- Rust host: `x86_64-apple-darwin`.
- Core executable: `Mach-O64/x86_64`; 25949200 bytes; SHA-256 `3e2fa33892a6dfa08fa42b3d6bdf43050191da435c0cd8131813c185311c5a7d`.
- Session test executable: `Mach-O64/x86_64`; SHA-256 `37bb40a5b84af979631bfdd1474a994afdbaa755c8ea90feb4c65fce7b028b8d`.
- Counts: 2 typed wrappers passed; 6 exact native selectors passed individually as 1/0/0.
- Report SHA-256: `12d3d325c793894951a57f6654cf2b9884e9f3ea0b1f38f399573a5df6a48e39`.

### Windows Server 2025 x86_64 (`windows-2025`)
- [Successful native job](https://github.com/678mjoe/JMCLCore/actions/runs/42388086740/job/128449654375)
- Observed host: `Windows-2025Server-10.0.26100-SP0`; machine `AMD64`.
- Rust host: `x86_64-pc-windows-msvc`.
- Core executable: `PE32+/x86_64`; 14053376 bytes; SHA-256 `15ed46fabf7174274317c0953ca137dce28b508265ce66162d9978cb2cd35bf5`.
- Session test executable: `PE32+/x86_64`; SHA-256 `e53c93619bc782ede8f0ddd5302424ae182b7e3a327173d9319426e6a27ec20f`.
- Counts: 2 typed wrappers passed; 6 exact native selectors passed individually as 1/0/0.
- Report SHA-256: `bdcdcb3f2b525810cd7fa8fe7965718db336d025faed413aab71ac8830962837`.

## Exact scope

Typed scripts: `test-account-core.ts` and `test-capabilities-core.ts`.

- `handshake_reports_protocol_v1`
- `ping_error_mapping_and_session_survival`
- `java_runtime_list_on_empty_store`
- `instance_status_matches_version_and_loader_but_not_source`
- `concurrent_callers_on_one_session_are_serialized`
- `account_metadata_requires_account_envelope_and_round_trips`

The standalone crate imports the pinned frontend `session.rs`, `transport.rs`, `modpack_preview.rs`, and full `rpc_smoke.rs` byte-for-byte. It executes only the six exact selectors above. Unknown/missing/duplicate inventory, zero tests, ignored tests, wrong names, wrong hosts, executable-architecture mismatch or modified subjects fail closed. The inherited Java detection, world/modpack and Windows PowerShell-console cases are not selected.

Typed wrappers receive the frozen Core absolute path; native smoke resolves the explicitly staged `backend-binaries/jmcl-core[.exe]` in its disposable checkout. Built/frozen/staged Core hashes match, and the native test artifact hash remains unchanged. Each recorded wrapper, build, inventory and exact-test command gets a distinct owned TMPDIR/TEMP/TMP root; its child processes inherit that root. All JMCL overrides are removed. Tracked source bytes and cleanliness are verified around execution. This does not claim cross-platform syscall-enforced network isolation.

## Initial run and narrowly scoped correction

The [initial run](https://github.com/678mjoe/JMCLCore/actions/runs/42387393639), harness `8aa5001997de7eecfd2621244613f12c761e6a80`, passed Linux and both native macOS jobs. Windows stopped in the pure recorder negative guard before Core build or either contract layer: Python 3.12 default cp1252 stdout could not display the intentionally malformed-byte replacement character. This was a recorder display failure, not a Core contract failure.

The [two-file fix](https://github.com/678mjoe/JMCLCore/compare/8aa5001997de7eecfd2621244613f12c761e6a80...d697c8fd12766979aed4c3190a3b19fc094b4f7d) escapes display output for the console encoding, preserves original bytes/hash and strict UTF-8 validation, and adds a cp1252 regression. Source pins, workflow, test selectors and scope are unchanged. The final run reran all four platforms and passed. Both runs remain available.

## Evidence retention

- Both run links above retain the original GitHub Actions job records. The final workflow intentionally uploaded no Actions artifact, package or executable.
- A separate evidence backup contains all 8 connector-decoded full job logs, all 7 successful reports, both reviewed harness versions, run/job metadata and checksummed output evidence.
- Each saved report is matched to the SHA-256 emitted by its runner. Windows-native CRLF was restored before matching its report hash.
- All 77 per-command raw output byte sequences from successful jobs were reconstructed from the decoded log display, then matched exactly to the recorded original SHA-256 and byte length. This provenance is explicit; the full job log downloads themselves are decoded text.
- The backup does not contain the four platform executable binaries. Their native format, length, hashes and exact build source are recorded in each report.

The experimental branch is `rust-frontend-contract-ci-20261107`. The original bounded-input branch, its allowlist/hash and workflow, production frontend pins, Core main, and release configuration were not changed. This results-only document is outside the new workflow path filter and does not change or rerun the executed harness.

## Remaining boundaries

No real accounts, OAuth/keychain, system Java detection/probe/execution, automatic Java acquisition/runtime archive installation, actual game/loader/server execution, Tauri GUI/IPC, bundle generation/installer execution, signing, release or deployment was performed by this lane. Passing these finite contracts does not cover those gates.

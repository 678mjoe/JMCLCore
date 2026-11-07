# Frozen native frontend contracts

This independent experiment runs exactly two existing typed-wrapper scripts and
six exact existing native Session smoke selectors on standard public runners:
Ubuntu 24.04 x86_64, macOS 15 ARM64, macOS 15 Intel, Windows 2025 x86_64.
It does not replace or modify the original bounded-input CI or generic bridge.

## Immutable subjects

- Core commit `bdd6cb3e55d69cb9e0e02037d251635e289207a8`, tree
  `0e8e901bc587cf3c6d7dc1ba78821c4701785e20`.
- Frontend commit `9c0fd81ce8109c1e35229560fee31308ca114ef5`, tree
  `48ea91981bdb7b375bca8e9bc47738b6c3dde4d1`.
- Rust `1.99.1`, Bun `1.4.4`; official action SHAs match already-reviewed
  `rust-frontend-bridge.yml` (checkout v6.0.2, setup-bun v2.2.0).
- Core fixture host-architecture correction is already in this frozen tree.
  This lane does not run that Java probe fixture or assert ARM acceptance for it.

Both repositories were read as public with the connected account's push access
before experiment preparation. The workflow additionally fails closed unless
its event repository is public and its ref is the exact experiment branch.
Only the new workflow/helper paths on `rust-frontend-contract-ci-20261107` trigger
this workflow. The frozen parent contains three other workflows: `quality.yml`
and `rust-rewrite.yml` accept pushes only to main/rust-rewrite; the old
`rust-frontend-bridge.yml` accepts pushes only to rust-rewrite. Their pull-request
and manual triggers are not invoked. No PR is opened and no old workflow changes.

## Preparation versus execution

The standalone crate avoids GUI/system dependencies. Its manifest and lock are
derived from the existing bridge crate at the frozen Core tree: dependencies
and lock remain unchanged, auto-discovered test targets are disabled, and the
sole declared integration target is `rpc_smoke`. The entry point exports only
`session`, `transport`, and `modpack_preview`. All three modules and the complete
smoke source are copied byte-identically from the pinned frontend. Building the
complete smoke file is not executing all its tests.

The preparation command verifies clean exact source pins and creates a new
`.native-contract-harness` inside a disposable frontend checkout. Registry
prerequisites are fetched with locked Cargo manifests. Bun installs its frozen
lock with lifecycle scripts disabled. Execution then uses offline, locked Cargo
for Core build, smoke no-run compilation, inventory and every selected test.
No Tauri crate build, frontend production build, package or installer is run.

The helper builds the real Rust executable from the fixed clean Core source into
an owned target directory, verifies ELF/Mach-O/PE architecture against actual
Python platform and `rustc -Vv` host, freezes and hashes its copy, and stages that
copy as the disposable frontend's `backend-binaries/jmcl-core[.exe]`. Typed
wrappers receive the frozen absolute path. The inherited smoke's `core_path()`
uses its compile-time manifest parent plus that exact staging path directly;
it does not rely on PATH, debug sidecar discovery, or the identity string alone.
All JMCL overrides and cross-target/alternate-compiler environment variables
are removed from each executed child environment.

Each wrapper, build, inventory and selected test gets a fresh owned temporary
root with TMPDIR/TEMP/TMP set to its native absolute path. This confines the
inherited fixture's initial remove-dir operation. Selected cases execute
sequentially. The full tracked source SHA-256 inventory and binary hashes are
checked around each selected test, and tracked source cleanliness is checked.
The compiled native test artifact is also architecture-checked and hashed.

The selector inventory must match the frozen full test inventory. Unknown,
missing or duplicate names fail. Execution commands can only be constructed for
the six named selectors, with `--exact --nocapture --test-threads=1`; every result
must contain the exact name, one running test, one pass, zero failures and zero
ignored. Zero-test Cargo success is rejected. Neither skip lists nor all-features
are permitted. Pure fake/negative regressions run normally and under Python -O.

## Audited execution scope

Typed scripts are `test-account-core.ts` and `test-capabilities-core.ts`. The
first saves/gets/lists/updates/deletes invented public account metadata in a fresh
working directory; no login, token or keychain is involved. The second tests
invalid parameter guards for policy/exclusivity and worker/retry bounds, then
lists an explicitly owned empty runtime store. Conflicting Java override/path
inputs stop before probing; invalid policy stops before install planning.

Six exact smoke selectors:

1. `handshake_reports_protocol_v1`
2. `ping_error_mapping_and_session_survival`
3. `java_runtime_list_on_empty_store`
4. `instance_status_matches_version_and_loader_but_not_source`
5. `concurrent_callers_on_one_session_are_serialized`
6. `account_metadata_requires_account_envelope_and_round_trips`

These use protocol/error messages, temporary instance manifests, a temporary
empty runtime store, serialization and synthetic local public account metadata.
The inherited Java detection, world lifecycle, modpack import and Windows
PowerShell console probe remain in the compiled inventory but are never selected.
No system Java detection/probe/execution/acquisition, runtime archives, games,
servers, real accounts or real network service calls are in the selected slice.
This is source-reviewed bounded scope, not a claim of syscall-enforced network
isolation across these hosts.

## Evidence and limits

Raw command output, argument arrays, temporary roots, source/host/version hashes,
binary and native-test hashes, exact counts, and log SHA-256 values are retained
in a fresh work directory and printed to Actions logs, including `report.json`.
No artifact, package, installer, release or deployment upload occurs.

Passing proves only these finite typed-wrapper and native Session/
LocalProcessTransport contracts against the frozen Rust candidate. It does not
prove Tauri IPC, mounted UI, full Tauri build, debug/release bundled discovery,
GUI launch, packaged distribution, live TLS/service behavior, real authentication,
game/loader/server execution or runtime acquisition. `replacement_allowed` and
`full_compatibility` stay false. Unrun platforms must not be described as passing.

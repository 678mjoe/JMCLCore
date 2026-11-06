# Local synthetic ordinary-instance handler chain

Scope: Linux-only, owned temporary directories, synthetic inert artifact bytes,
fully prepopulated metadata cache and CAS. No real account, instance, game,
network request, Java process, loader processor, ZIP/JAR parsing or extraction.
The artifact has a `.jar` destination only because that is the production client
path contract; its bytes are plain inert text. Both metadata and artifact
transport implementations panic on any attempted fetch.

`install::vanilla::synthetic_chain_tests` covers:

- Real protocol request/response serialization for instance create/get/list.
- Production vanilla execute handler with injected offline transports, then
  production prepare parser/runner, with no natives or asset projections.
- Two instances transition from not installed to installed, use the same CAS
  inode/device, have read-only shared artifacts and distinct receipts.
- File/progress events and exactly one final result, cached hard-link placement,
  no retry, no download and zero transferred bytes.
- Dropping/recreating the metadata Service, repeating cached execute, and fresh
  instance get/list reads retain installed state. Removing one synthetic
  receipt affects only its owner, not the other instance or shared CAS.
- A synthetic file blocking the runtime directory makes prepare emit one
  PREPARE_FAILED terminal error, preserving its canary and completed download
  receipt. Installed means a matching download receipt, not successful prepare.

Run only this bounded fixture locally:

```
cargo test --offline --locked --features local-synthetic-chain-tests --lib synthetic_chain_tests
```

This is a handler-chain regression test, not full application acceptance. It
uses an existing test-only injection seam and directly invokes the production
prepare runner to avoid constructing HTTP transports. It does not validate the
top-level install dispatcher, a restarted native Session/process, frontend task
state, Tauri IPC, real downloads, Java acquisition, archive preparation, game
launch, or other platforms. No production API refactor or CI scope change is
included. Real GUI success acceptance remains outstanding.

The module is behind the default-off `local-synthetic-chain-tests` feature so
existing CI `--all-targets --include-ignored` commands do not execute it.
This prevents inclusion in broad test invocations that do not explicitly enable
the feature. Invocations with `--all-features` enable it and run these tests on
Linux. The older
workflow's push trigger only names `main` and `rust-rewrite`; publishing a new
experimental branch does not by itself trigger that workflow. The separate
bounded CI allowlist is unchanged.

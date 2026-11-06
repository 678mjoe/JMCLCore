# Local native content lifecycle

This explicit local lane extends the already isolated native Session harness;
it does not repeat the ordinary-instance installation chain. It imports the
same unchanged frontend `c0107f1803c73398a99f8ddd27ed712c0ed2e1b4` modules and
runs one separate exact test under the unchanged `network_guard.py`.

```
python rust/compat/frontend_bridge/run_instance_chain.py \
  --scenario content --frontend /path/to/clean/frontend \
  --rust /path/to/jmcl-core-rust
```

The default-off feature is `local-native-content-lifecycle`. Neither the old
bridge runner nor CI enables it. As with the instance lane, `--all-features`
would enable it; supported execution goes through the guarded runner, which
verifies the exact test inventory and requires one passed/unignored test.
Builds are offline/locked and `--build-only` never starts Core.

## Safety and actual coverage

Both synthetic instances are created in a new owned temporary directory with
`validate:false`. The files `mods/manual.jar` and `resourcepacks/manual.zip`
contain only `hello`: their extensions select content families; no code reads
an archive, executes Java, resolves metadata or performs a provider request.
Content adoption hashes ordinary bytes and imports/materializes through CAS.
Toggle/remove manipulate local paths and registry entries only.

The test uses real native Session and the real production dispatcher to check:

- list unmanaged files; adopt them as manual entries; repeat adoption with no
  new entries and byte-identical registry;
- two families in two instances share a read-only CAS inode/device;
- manual set-version returns CONTENT_NOT_FOUND, preserving registry/file;
- enable on enabled content is unchanged; disable and enable return true then
  false on repetition; registry paths stay canonical while disk uses .disabled;
- disabled state survives close/reopen into a new Core child;
- removal covers both an enabled entry and a disabled entry; repeated removal
  returns CONTENT_NOT_FOUND, rather than inventing an idempotent success rule;
- the second instance's registry and bytes remain unchanged, the CAS object
  remains with the expected links, and another new Core child sees final state.

The set-version rejection was source-reviewed before execution:
`content/remote.rs` filters out `Provider::Manual` while selecting a registry
entry and returns `ContentNotFound` before its first metadata transport call.
The dispatcher maps that error to `CONTENT_NOT_FOUND`. The filter also makes
any accidental attempt to create a network socket fatal during the test.
No provider IDs are added to manual entries to force an artificial match.

All guard self-tests remain mandatory immediately before execution. This is
trusted-process local isolation, not a general sandbox for hostile programs.
See INSTANCE_CHAIN.md for the guard's explicit scope and limitations.

This proves a native/Core local content lifecycle, not the mounted content UI,
Tauri IPC, provider installs/upgrades, actual archive compatibility, Java,
other platforms or replacement readiness. No production behavior is changed.

Local results are recorded in `content_lifecycle_evidence.json`: guard self-tests
passed and the exact content test passed (1 passed, 0 failed, 0 ignored).
Python compilation, Rust formatting and diff checks passed. Default Clippy
completed with the same five existing warnings in unchanged frontend
transport.rs; no warning was reported for the new test. Strict warning-free
status is not claimed, and those imported sources were not modified.

# Vanilla prepare RPC byte routing, 2026-11-03

Base: experimental recovery `392564305cea1a3d025523c3982937bce89163c3`.
This is newly reconstructed work from saved source, not recovery of the unpushed cloud task.

The vanilla-only RPC route now preserves byte-array version identities and directory paths through metadata planning and game-native/resource preparation. The separate prepare schema ignores execute-only fields, validates before opening a root, and preserves original planning/root/preparation error precedence. Loader routes remain unchanged. Preparation does not download planned files, acquire Java, or write installation receipts.

Local Linux checks:

- Seven focused prepare tests: schema defaults, invalid-field precedence, raw identity/path success, existing string-route comparison, public invalid-request routing, blocked root, and missing native cache file.
- Forty-six existing plan tests passed, including existing resource projection and benign game-native preparation fixtures.
- Sixteen invalid-parameter black-box comparisons match the pinned Zig oracle exactly for exit status, stdout, stderr and no filesystem writes. Reproduce with `python rust/compat/prepare-byte-schema.py --zig /path/to/pinned/core --rust /path/to/rust/core`.
- `cargo fmt --check`, offline locked all-target Clippy with warnings denied, and binary build passed.

Oracle checkout: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`; oracle binary SHA-256 `0d49ff4130720c67397c3f673afaaf226f4b1f8384f68ea599e43340030c164b`.

These are focused synthetic/local checks, not a full-suite, Windows/macOS native, real-download, authentication or Minecraft launch acceptance. CLI byte-argument adaptation remains separate. The experimental recovery branch still contains other unintegrated work; this record does not establish compatibility or replacement readiness of that combined branch. Production main is unchanged.

## Native CLI continuation

The native argv bridge additionally claims vanilla `install prepare` requests containing non-UTF-8 arguments. It preserves raw version/directory bytes and architecture aliases; validates all option pairs before handing valid Fabric/NeoForge selections back to the legacy path; and preserves the pinned CLI's parsed-but-unused Forge option. Ordinary UTF-8 requests keep their previous path.

The combined vanilla suite passes 16 tests, including two injected CLI execution cases and argument/error dispatch. Nine raw POSIX argv rejection cases match the pinned Zig binary byte-for-byte and leave no files (`rust/compat/prepare-byte-cli.py`). All-target Clippy and the binary build pass. These CLI fixtures were run on Linux only. Valid Fabric/NeoForge selections containing non-UTF-8 argv still use the legacy path and remain a known unsupported gap; this work does not claim their completion.

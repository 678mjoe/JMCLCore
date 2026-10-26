# Server content LIST validates the existing display-name read view

This separate slice starts at central `3cef80d5b4ce1226d8e4e853194897db1728d7d3`
after the server LIST target-byte prerequisite. Only list_rpc changes: validate
with existing `servers::store::load_read`, discard the validated read manifest,
then open the same anchored server handle. Display names never become operational
Strings. Non-list target/target_from_root, Params parsing, lifecycle and store
source are byte-for-byte unchanged. The Linux gate and existing world(false)
path remain unchanged. No lock, state, PID, signal or process operation occurs.

## Source and proof

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e` validates a persisted
byte-string display name before opening the content directory. The previous
String consumer rejected even ASCII name arrays. A 72-case pure-read discovery
corpus found 20 mismatches: successful names were refused, and invalid stored IDs
combined with byte-array names lost source SERVER_NOT_FOUND precedence.

An actual Rust LIST unit was red before this change. Baseline binary SHA256:
`2ea3e7910f55ffd92b10b4a26c58f50cc2abda84f0e19d8e28c79f1c5cf8ba28`.
The expanded oracle matrix has 126 paired LIST cases: String, ASCII/Unicode/FF/
control arrays, empty/overlong/overflow/null names crossed with valid, missing-
version, invalid-ID, ID-mismatch, missing-version-plus-invalid-ID, schema and
source controls. Structural completion still precedes semantic validation;
source error mapping is retained. Both server mods and datapacks LIST are tested.

Every fixture is owned and has no process state, with normal server.properties,
opaque sibling/hardlink peers, and full bytes/device/inode/mode/mtime/link-count
snapshots before/after each call. No root/world/registry creation or rewrite is
permitted. Source acceptance of control bytes is display metadata behavior, not
permission to use those bytes in filesystem paths or operational fields.

```sh
cargo test --manifest-path rust/Cargo.toml --test server_content_list_display_bytes
python3 rust/compat/server_content_list_display_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

All 126 paired cases and the new unit pass. Existing 212-case target-byte corpus
and its two Linux units pass; formatting, all-target Clippy and diff checks pass.
Green binary SHA256:
`dbeef0d67ef0ad20453a1a0ad963dd37e2e7fcf2f37cc2c70edded139118a4ca`.
Native non-Linux execution is not claimed; the script checks only the existing
unsupported-platform gate there. No non-list mutation, installation, Java,
archives, network, URL transport or remote write is part of this change.

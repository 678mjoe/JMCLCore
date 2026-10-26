# Local content project-selector byte arrays

This slice changes only local enable/disable/remove request parsing. It does not
change provider acquisition arguments, installation, adoption, archives or Java.
The pinned baseline is Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

`src/protocol/content.zig::ContentRemoveParams` and the server/datapack request
schemas declare `project: ?[]const u8`; `parseFromValue` accepts strings and
dynamic-u8 arrays. `service.zig::setInstanceContentEnabled` and removal validate
project references only after opening the relevant root. The server paths first
resolve the target, acquire the stopped guard and resolve the world. The project
validator in `src/content/modrinth.zig::validateProjectRef` permits only 1–64
ASCII alphanumeric, hyphen, underscore or dot bytes.

Previously `[97,98,99]` failed Rust parameter parsing while Zig reached the
expected no-match result. Successfully decoded opaque/empty arrays likewise
must reach selector validation rather than parse failure. A blocked root takes
precedence over those semantic errors. Numeric elements retain dynamic binary64
coercion, while quoted numeric tokens retain the existing exact numeric grammar;
raw `97.0000000000000001` and its quoted form intentionally differ.

Client private parameters now retain project bytes. Server parameters keep the
existing `project: Option<String>` for remote operations and CLI, with a separate
local RPC byte slot. Only after root/world/guard setup does a private helper
validate the bytes and borrow an ASCII `&str` for unchanged public Root APIs.
There is no lossy conversion, native-path reinterpretation or storage API change.
The existing String CLI behavior is preserved. Provider execution and request
construction code is unchanged.

A separate, source-proven diagnostic correction is included: invalid project
references in client datapack mutations use `Project, version, or Minecraft
version identifiers are invalid`, as in the source's `writeWorldDatapackError`.
Plain String `""`, `"bad/ref"` and `"中文"` controls establish this independently
of array acceptance. Other file-selector errors retain their existing mapping.

## Bounded executable evidence

`content_project_selectors.py`, run against the exact local binary and pinned
oracle, checks on Linux:

- **1,836 exact RPC comparisons** across four client and two server families,
  three operations, numeric grammar, invalid bytes/types, empty/null selectors,
  project/file conflicts, blocked roots, invalid server IDs, blocked guard files
  and missing/invalid worlds. All these cases have empty registries, require an
  error, and preserve complete bytes/modes/mtime/inode/nlink snapshots including
  sibling hardlinks. No process state or running server is supplied.
- **36 exact CLI controls** use ordinary String projects and preserve disk state.
- **12 positive comparisons** separately exercise client/server mod selection:
  String or ASCII-array project, disable → enable → remove. They assert the
  actual selected entry, enabled flags, exact registry/output/files, and unchanged
  sibling/peer bytes, mode, mtime and identity. The target's removal reduces the
  shared link count by exactly one. Files are ordinary opaque local fixtures,
  not executed or acquired. Positive datapack mutation is not claimed by this
  slice; the no-match datapack grammar/error cases remain covered.

The exact baseline binary fails the new positive array-grammar assertion at
`mods.enable [97,98,99]`; the patched binary passes. A focused pure unit test
covers the private byte/semantic boundary. The existing **558** paired file-array
selector cases also pass unchanged. Native macOS/Windows execution is pending
central CI; server runtime cases are Linux-only by the existing platform gate.

```sh
python rust/compat/content_project_selectors.py \
  --zig /path/to/pinned/jmcl-core --rust rust/target/debug/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
cargo test --locked --lib local_project_byte_parse_and_semantic_boundary
JMCL_ZIG_SOURCE=/path/to/pinned/source JMCL_ZIG_ORACLE=/path/to/pinned/jmcl-core \
  cargo test --locked --test content_selector_bytes -- --ignored --nocapture
```

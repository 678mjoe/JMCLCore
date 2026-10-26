# Instance deletion validates display-name bytes without rewriting metadata

Instance deletion now uses the existing byte-preserving `ReadManifest` validation
view: both platform `Root::delete` methods call `load_read`, and the RPC delete
preflight does likewise. The three changed calls preserve the existing anchored
load/traversal and `remove_tree` implementations. No manifest rewrite, save API,
String conversion, path reconstruction, symlink policy or deletion primitive is
introduced. Operational `Manifest`, `Root::load`, creation and installation are
unchanged. CLI already delegates to `Root::delete`.

## Source and actual baseline

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/instance_store.zig:136`, validates a byte-name manifest before deleting its
owned subtree. Display names accept any nonempty byte sequence up to 256 bytes;
NUL/newline acceptance is metadata behavior, not a filesystem path relaxation.

Baseline Rust ab68db82e72815cea10fc9b7894579a501e667ef (binary SHA256
4b1e18ca47670f7700bc12c5eb45c19428dc445e709fc4f0343cd345f8701632) refused ASCII,
UTF8, FF, NUL and newline arrays with UnexpectedToken, where Zig deleted them.
Empty-array CLI diagnostics also differed (source InvalidInstanceName). An
actual Rust CLI-delete regression was red before the three-call change.

## Bounded fixture evidence

`instance_delete_display_bytes` invokes only instance.delete through actual Rust
CLI/RPC handlers and the pinned executable. Successful deletion removes only the
owned instance subtree. A sibling sentinel and hardlink peer outside that subtree
retain bytes, mtime and permissions; Unix additionally checks device/inode/mode
and the exact one-link decrement. Rejected requests preserve the complete file
snapshot. No symlinks or real user directories are introduced.

The matrix covers normal strings/ASCII arrays/opaque bytes/control bytes, empty/
oversized/invalid typed names; schema/version/source/ID/loader errors, missing
fields, invalid/missing/empty request IDs, and invalid-root versus ID ordering.
RPC payloads are compared structurally, CLI exit/stdout/stderr byte-for-byte.

Local results: 2 new tests and 52 exact paired delete cases pass; 2 named
read-view unit tests and the existing CLI manifest-error read corpus pass.
Formatting, all-target Clippy and diff checks pass.

```sh
cargo test --manifest-path rust/Cargo.toml --test instance_delete_display_bytes -- --include-ignored
```

Set `JMCL_ZIG_ORACLE` to the pinned executable. Local execution is limited to
these disposable deletion fixtures and named read/validation regressions; no
server lifecycle, downloads, installs, Java, archives or network. Windows source
uses the same narrow bridge; native Windows acceptance remains a separate gate.

# Opaque content adoption: byte filenames

## Boundary

Adoption now loads the existing byte-preserving registry view and retains unrelated
raw registered paths when atomically saving the complete validated document. Only
`Adopted.file` changes representation: UTF-8 remains a JSON string, other bytes an
integer array; text CLI writes the original bytes. Public String load/save,
remove/toggle and remote installation APIs are unchanged. Raw registry mutations
outside adoption remain a separate outstanding compatibility stage.

Content directory/file lookup retains the existing followed-alias behavior. The
CAS bridge uses a distinct crate-private byte-relative parent with an explicit
`create` flag, the original no-follow component operations and the existing digest
transaction. Source lookup never creates parents. No ambient path reconstruction,
new copy fallback, archive/native extraction caller or acquisition path is added.
Existing documented cross-device behavior is untouched. Same-volume fixtures must
retain shared inode identity, immutable bytes and readonly status.

Windows enumeration converts UTF-16 to exact WTF-8, preserving unpaired surrogate
names instead of dropping them. The existing strict byte-to-native converter is
reused. Arbitrary non-WTF-8 bytes such as FF remain valid registry record data but
cannot become native Windows paths. Windows physical-name proof is a native CI
gate, not established by the Linux run or by the roundtrip unit test.

## Source and evidence

Oracle: unchanged Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/service.zig:3181` (`adoptInstanceContent`),
`src/content/content_registry.zig` (ordered validation/atomic save), and existing
CLI/protocol content handlers. The new test verifies the configured source pin
and a clean `src` tree; the configured executable must be built from that source.

Baseline failed with `CONTENT_ADOPT_FAILED` when an ordinary unmanaged file
coexisted with an unrelated raw registered path. Final Linux evidence:

- `content_adopt_bytes`: 2 tests, 73 exact paired invocations: four client and two
  Linux server families, RPC and text CLI, UTF-8 / WTF-8 / FF physical names, first
  adoption and repeated empty adoption; plus a zero-byte failure. Exact registry
  bytes, SHA1, CAS/source/peer identity, readonly status and retained raw records
  are asserted. No content is parsed as an archive.
- `content_byte_bridge_tests`: missing-parent source lookup cannot create a
  directory; unsafe components rejected; CAS source directory alias rejected
  without changing its target.
- Existing adoption tests: 3 pass, including 120 RPC/disk and 28 CLI/disk pairs.
- Existing registered byte listing: 4 oracle tests / 166 local paired surfaces
  pass. Its String-only legacy-view control remains applicable.
- Format check and all-target Clippy pass.

Physical-name fixture creation excludes only platform EILSEQ (checked using the
platform errno, not a hard-coded Linux number), reporting the exact error and
checking path absence/unchanged registry. All other failures fail the test.
Registry retention with ordinary native files remains mandatory on every platform.
The Windows fixture uses an unpaired surrogate; Unix additionally covers byte FF.
No Java, download, archive parser, real user data or workload is invoked.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_adopt_bytes -- --ignored --nocapture
cargo test --manifest-path rust/Cargo.toml --lib content_byte_bridge_tests
cargo test --manifest-path rust/Cargo.toml --test content_adopt -- --include-ignored
```

Set `JMCL_ZIG_SOURCE` and `JMCL_ZIG_ORACLE` to the pinned checkout and its binary.
Native CI must also run the Windows-only `filename_wtf8_is_exact_and_roundtrips`
unit test. Integration must review and repin the conservative whole-file dispatch
audit hashes for changed routing files; handler associations themselves are not
changed by this implementation.

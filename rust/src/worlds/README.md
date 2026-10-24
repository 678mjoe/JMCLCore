# Incremental worlds port

Implemented with Unix and Windows adapters; locally exercised on Linux: the 11 client `worlds.*` RPC handlers,
NBT metadata, private (non-hardlinked) duplication, locked-world mutation refusal,
ZIP import/export, backup listing/deletion and restore. The pinned Zig oracle
reads Rust exports, and Rust reads Zig exports. Archive compressed bytes may vary;
entry data and metadata are the interoperability contract and every reported
`size_bytes` is checked against the actual generated file.

Run the oracle tests explicitly, in addition to ordinary unit tests:

```
JMCL_ZIG_ORACLE=/absolute/path/to/pinned/jmcl-core cargo test -- --include-ignored
```

This is **not full worlds compatibility** yet:

- The native Windows filesystem adapter uses shared handle-relative NT primitives.
  Cross-compilation and lint pass; native execution is still required. Ordinary
  private-copy, lock, archive, rollback, and NBT tests are enabled on Windows too.
  `instance worlds` CLI is oracle-tested for read/validation, rename, duplicate,
  deletion, import, restore, and backup deletion output. Export size and backup
  timestamp-bearing CLI output still require broader corpus coverage.
- Unix descendants are opened descriptor-relatively without following symlinks.
  Metadata `level.dat` read aliases are preserved, with atomic edits replacing
  the alias entry rather than modifying its target; export still rejects links.
  Other directory/file alias cases are not yet a parity claim. Ordinary
  configured root paths may traverse ancestors just as before.
- The ZIP dependency verifies CRCs. The original ZIP reader does not perform the
  same check. Rejection of malformed CRC archives is a tracked compatibility
  exception, not silently normalized by oracle tests.
- Running-world detection uses the original probe-and-release lock model. It
  rejects a lock held at the probe, but is not exclusion against a game starting
  afterwards. Callers must continue serializing mutations.
- Real game/native-window/server acceptance has not been run.

Negative tests cover malformed/truncated/deep NBT, negative item lengths,
path traversal, over-limit declared archive entries, failed publication rollback,
cleanup of temporary state, and filesystem symlinks in duplicate/export. They are
not exhaustive fuzzing or power-loss tests. Coupled Linux server dimension-world
backup/restore remains a separate server-lifecycle slice, not these client APIs.

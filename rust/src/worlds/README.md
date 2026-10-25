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
  deletion, import, restore, and backup deletion output. The bounded
  `compat/world_cli_archives.py` corpus additionally covers export replacement,
  actual printed archive size/file count, five backup-label cases, timestamped
  filenames, plain/JSON backup listings, and three failure cases on Linux.
  This is not exhaustive CLI or native-platform acceptance.
- Explicitly selected existing world-directory aliases follow the original
  `openWorld`/`openMutable` contract: their resolved directory is the requested
  world, including requested mutations. Listing does not enumerate aliases as
  worlds. `level.dat` and `icon.png` reads and `session.lock` probes follow their
  named leaf aliases. Atomic metadata edits replace the level.dat entry rather
  than its target; a lock probe never writes lock bytes. Renaming/deleting a world
  alias acts on that directory entry, not its target directory entry.
  Recursive duplicate/export still reject symlink entries encountered inside a
  world. Archive-created links are not followed: extraction uses private staging
  and no-follow descendant opens. Ordinary configured root paths may traverse
  ancestors just as before. Native Windows alias execution still needs CI.
- CRC metadata is intentionally not an integrity gate in the source ZIP reader.
  Stored/deflate world import preserves this acceptance (oracle tested with
  unchanged payload and altered CRC fields), while retaining path/encryption,
  declared and actual size, structural and decompression checks. This is source
  compatibility, not a claim that CRC strictness itself was a source defect.
- Running-world detection uses the original probe-and-release lock model. It
  rejects a lock held at the probe, but is not exclusion against a game starting
  afterwards. Callers must continue serializing mutations.
- Real game/native-window/server acceptance has not been run.

Negative tests cover malformed/truncated/deep NBT, negative item lengths,
path traversal, over-limit declared archive entries, failed publication rollback,
cleanup of temporary state, and filesystem symlinks in duplicate/export. They are
not exhaustive fuzzing or power-loss tests. Coupled Linux server dimension-world
backup/restore remains a separate server-lifecycle slice, not these client APIs.

## Timestamp-bearing CLI corpus

Run from `rust/` with both compiled binaries and their independently recorded
SHA-256 values (the runner verifies and privately copies them before execution):

```sh
python3 compat/world_cli_archives.py \
  --zig /path/to/pinned/jmcl-core --zig-sha256 "$ZIG_SHA256" \
  --rust /path/to/jmcl-core-rust --rust-sha256 "$RUST_SHA256"
```

The corpus checks 48 command results across nine paired synthetic scenarios.
Every stdout/stderr/status is checked; archive entry sets, bytes, fixed DOS
metadata timestamps, exclusions and private source-file identity are checked on
disk. Each displayed size is checked against that implementation's actual
archive, never rewritten to hide compressor differences. Generated filename
UTC timestamps must parse and fall between the command's sampled start/end
seconds before they can be abstracted. Listing `created_ms` must equal the
archive's exact on-disk modification time; a controlled subsecond mtime verifies
that it is not inferred from the filename. Only the validated filename timestamp
is abstracted across runs. No live game, Java runtime, network, or external data
is used. Linux execution is verified; native Windows/macOS execution of this
additional standalone corpus is not claimed.

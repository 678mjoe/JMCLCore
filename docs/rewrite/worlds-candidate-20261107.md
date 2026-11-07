# Client-world normal-flow verification, 2026-11-07

The reviewed local client-world slice passed: **7 exact Rust library tests**, each
with 1 passed / 0 failed / 0 ignored, plus the existing native CLI archive corpus
with **9 paired scenarios and 48 command results**. No new production defect was
observed. This evidence-only change modifies no production implementation, test
fixture, dependency, or CI workflow. It is not full replacement acceptance.

## Exact subject and prerequisites

- Executed clean local commit: `009b046ac0a4916c0fa5b073212cc7404db2e51a`.
- Executed tree: `5e61790fde937ab927b2b79d1a59d3323f2dbaa3`.
- The coordinator supplied remote commit `2727ce6428042c1b1bd2952419c6b13f576f7f8b`
  with that same tree. Its commit object was unavailable locally. This run used
  the verified local commit above and did not fetch or query the remote.
- Linux x86_64, Rust/Cargo 1.99.1. All Cargo commands used `--offline --locked`,
  default features only, the existing registry cache, and a new dedicated target
  directory. The exact paths, tool versions, commands and hashes are recorded in
  the [manifest](../../rust/compat/evidence/worlds-candidate-20261107/manifest.json).
- Zig 0.16.0 rebuilt the oracle from a clean, separate checkout at
  `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, tree
  `3e2b7c3e644c0da57334eba6eb8473613a81df8c`. Its cache and output directories
  were dedicated to this run. `zig build` only compiled the executable; it did
  not run Zig's test suite. The source has no external package dependencies.
- Both source checkouts were checked clean before and after execution. The two
  oracle tests explicitly used `JMCL_ZIG_ORACLE` and `--ignored`; no skipped
  oracle case was counted as a passing test.

Rebuilt executable SHA-256:

- Rust: `dad858d76f05cc4fad65814b03790a83e02e9244c5fa1d3155f882b89c1fd1f5`.
- Zig: `bf624b6a77bc83c2811748659efb9311e34ee036490ea50831d940bed710f39e`.

## Reviewed scope and results

Each test body, its fixture helpers, world dispatcher and relevant filesystem /
archive implementation were inspected before selection. Every world, archive,
icon, registry and lock file used here was synthetic and inside an owned
`TempDir`; no user save or external archive was read. The generated ZIP data was
world data only, not a Java runtime archive. No selected path discovers, downloads,
installs, probes or executes Java, starts a game/server, opens a socket, or uses an
account/provider. Real file operations, gzip/NBT processing, ZIP compression and
extraction, and an owned file lock were deliberately exercised.

The common command prefix was:

```sh
cargo test --manifest-path rust/Cargo.toml --offline --locked --lib
```

Append one fully qualified name below, then `-- --exact --nocapture`. The last
two names also require `--ignored` and the rebuilt pinned oracle environment.
The recorder requires the exact passing name, exactly `running 1 test`, and
exactly one result with 1 passed / 0 failed / 0 ignored. A zero-test command fails
the recorder rather than becoming a green result.

1. `worlds::tests::metadata_and_rename_preserve_unknown_nbt`: valid generated
   gzip/NBT metadata, matching version and creative mode, rename on disk,
   updated `LevelName`, preserved unknown NBT values.
2. `worlds::tests::mutable_duplicates_are_copies_not_hardlinks`: duplicate has
   a different actual inode; changing and deleting the copy leaves original
   region bytes intact.
3. `worlds::tests::locked_restore_refuses_and_stale_lock_allows`: managed backup,
   refusal while an owned session lock is held, no published extra world, then
   successful restore after unlock with the stale lock excluded.
4. `worlds::tests::export_excludes_registry_lock_and_temporary_files`: archive
   excludes private registry, lock and temporary entries; declared file count
   and reported size match the generated archive.
5. `worlds::tests::ordinary_zip_roundtrip_preserves_files`: export/import into a
   new world preserves region bytes and rejects an existing destination.
6. `worlds::tests::zig_rust_metadata_and_archive_interoperability`: exact
   semantic RPC-result equality for list/get, Zig export read by Rust, Rust
   export read by Zig, restored payload and metadata, and each archive's actual
   size. Rust calls its production handler/API; Zig runs as a Core subprocess.
7. `worlds::tests::zig_rust_world_cli_mutation_results_match`: nine paired CLI
   cases covering rename, duplicate, delete, import, restore, backup deletion,
   and three existing error cases. It checks status/stdout/stderr exactly;
   Rust runs its CLI adapter in-process, Zig its executable. This test alone
   does not assert every mutation's on-disk result.

The unchanged `rust/compat/world_cli_archives.py` then privately copied both
hash-verified Core executables and ran their actual CLI entry points. Its 48
results cover export replacement, five existing backup-label cases, plain/JSON
backup listings and three error scenarios. It checks archive entry names/bytes,
fixed ZIP timestamps, exclusions, actual size/file count, unchanged source
bytes/inodes/mtimes, bounded generated UTC filename times, and `created_ms`
against exact filesystem mtime. Only a generated filename timestamp already
validated against its execution window is abstracted. Compressed byte streams
and archive lengths are not required to match each other.

There is no `worlds.create` method in the contract. Initial saves here were
created as synthetic fixture files; new worlds were produced by duplicate or
import. This does not claim Minecraft world generation or a running-game save.

## Boundaries and evidence

No full suite, `--all-features`, Java acquisition/runtime installer, live account,
network/service, actual game/server, GUI/Tauri, Windows or macOS operation ran.
Existing malformed-input, symlink and unrelated server tests were not expanded
or blanket-run. These finite normal-flow checks do not establish arbitrary
archive compatibility, crash/power-loss recovery, or cross-platform parity.
No permission denial occurred; no remote write was made.

The [evidence directory](../../rust/compat/evidence/worlds-candidate-20261107)
contains unedited compiler/test output with command and return-code framing,
individual SHA-256 hashes in `manifest.json`, source fingerprints and the exact
run recorder. The recorder is a record of the original absolute paths and clean
executed commit, not a generic new acceptance lane. Its invocation required
sourcing `jmcl-tools/rust-env.sh`, overriding `CARGO_TARGET_DIR` with the dedicated
worlds target, and setting `TMPDIR` to the owned fixture directory shown in the
manifest. Build commands are retained in the three build logs.

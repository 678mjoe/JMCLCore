# Content-Addressed Instance Storage

JMCL separates immutable game artifacts from the per-instance Minecraft tree.
It uses each artifact's verified SHA-1 as its identity, stores that object once,
and materializes the paths required by every instance as hard links where the
filesystem permits.

This is not a conventional per-instance download cache. An instance owns its
mutable game data, while its client JAR, libraries, assets, native archives, and
logging configuration are views over one shared immutable object repository.

## Layout

The default shared repository is `.jmclcore`:

```text
.jmclcore/
  objects/
    sha1/
      ab/
        abcdef0123...
  locks/
    sha1/
      abcdef0123....lock

instances/
  fabric-1.20.4/
    .minecraft/
      libraries/... -> hard link to an object
      assets/...    -> hard link to an object
      versions/...  -> hard link to an object
      saves/        # owned by this instance
      logs/         # owned by this instance
      config/       # owned by this instance
```

The object filename is the lowercase SHA-1 digest. Directory fan-out by the
first two hexadecimal characters avoids a single large directory. A file with
the same SHA-1 can be shared by different Minecraft versions, Fabric profiles,
and distinct instances without relying on filenames or source URLs.

## Installation Transaction

For every planned file, the installer performs the following sequence:

1. Verify the instance destination by exact byte size and SHA-1, without
   following symbolic links.
2. Acquire an exclusive lock for the digest, then verify the shared object
   while that lock is held.
3. If the instance already has a valid file but the object repository does not,
   import that file into the repository with a hard link where possible.
4. Otherwise download into an atomic temporary file, enforce the expected byte
   count while streaming, verify the digest, and atomically publish the object.
5. Materialize the object at the instance path with a hard link or fallback
   copy.

The object lock is held across shared-object verification, import, download, and publication.
This makes object creation single-writer while allowing unrelated digests to be
installed concurrently.

## Atomic Materialization

On macOS and Linux, JMCL creates a uniquely named temporary hard link inside the
destination directory and renames it to the final path. Since the temporary and
final paths share a directory handle, the rename is atomic and an interrupted
installation cannot expose a partially copied artifact.

Windows uses a separate replacement path because its hard-link and open-file
semantics differ. A valid existing destination is retained. An invalid one is
made writable, removed, and replaced with a new native `CreateHardLinkW` link.

When a hard link cannot be created because source and destination are on
different filesystems, the filesystem does not support hard links, permissions
deny it, or a link quota is reached, JMCL copies the already verified object to
the destination. The copy remains verified but is not an atomic replacement;
later installation verifies it before reuse. Install results identify how many
objects were hard-linked and copied.

## Immutability Boundary

Published objects and their instance links are read-only. Because hard links
share an inode, mutating a linked static artifact would otherwise affect every
instance using that digest. JMCL therefore never repairs or upgrades an object
in place: it verifies or publishes a complete replacement object, then replaces
only the affected instance path.

Mutable runtime data is deliberately excluded from the repository:

- saves, screenshots, logs, options, and resource packs;
- `mods/` and mod configuration;
- extracted native files in the per-launch runtime directory;
- account data and credentials.

## Cache Reclamation

Deleting an instance removes only its directory entries. A shared object remains
available to other instances until it has only the store link remaining. Run
`store gc` to reclaim those cache-only objects:

```sh
jmcl-core store gc --store-directory .jmclcore
jmcl-core store gc --store-directory .jmclcore --execute
```

Collection defaults to dry-run mode and a 24-hour grace period. It holds the
same per-digest lock as installation during `--execute`, then deletes only
canonical regular files whose link count is one. Dry-run is read-only and
reports an unlocked snapshot. This is cache reclamation rather than logical
instance reachability: copied instance files do not retain their source object,
but they remain valid after that object is removed.

## Security Properties

- Every object identity is a validated 40-character SHA-1 digest and every use
  rechecks the expected digest and size.
- Object and destination access is constrained to directory handles with
  symbolic-link following disabled.
- Downloads are written atomically and are rejected on overflow, short body, or
  hash mismatch.
- Native extraction is separate from object storage and has its own path and
  archive validation rules.
- Windows path conversion uses directory-handle-derived paths and native wide
  APIs; positional reads used for hashing avoid the Zig 0.16 Windows I/O issue.

The SHA-1 value comes from the selected upstream metadata and is an integrity
identifier for the existing Minecraft ecosystem. It does not add a signature or
replace trust in the metadata source.

## Operational Consequences

- A second instance can reuse static artifacts immediately and should mostly
  report cached objects plus hard links.
- Choosing a shared store on another volume still works, but each instance file
  falls back to a copy and consumes local disk space.
- The store can be backed up or moved as ordinary files; instance links are
  recreated by running installation again if needed.
- Garbage collection is conservative: unrelated external hard links can retain
  objects, while copied instance files do not.

For command usage and response fields, see `docs/artifact-store.md`.

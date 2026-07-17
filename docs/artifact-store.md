# Shared Artifact Store

JMCL stores verified immutable files in a content-addressed repository and
materializes the Minecraft paths required by each instance with hard links.
The default repository is `./.jmclcore`:

```text
.jmclcore/
  objects/
    sha1/
      ab/
        abcdef...
  locks/
    sha1/
      abcdef....lock
```

The object path is derived only from a validated SHA-1. Every object is checked
against its expected length and digest before use. Published objects and their
instance links are read-only.

## Installation

```sh
jmcl-core install execute 26.1 \
  --directory .minecraft \
  --store-directory .jmclcore

jmcl-core instance install vanilla-26.1 \
  --store-directory .jmclcore
```

Installation uses the following order:

1. Verify the instance destination and store object.
2. Lock the object digest across processes.
3. Import an existing valid instance file when possible.
4. Otherwise download, hash, and atomically publish the object.
5. Create a temporary hard link in the instance and atomically replace the
   final destination.

If hard links are unavailable, cross a filesystem boundary, or exceed the
filesystem link limit, JMCL uses an atomic copy instead. CLI and RPC results
report `hardLinked`/`files_hard_linked` and `copied`/`files_copied` counts.

Linux and macOS use the Zig directory hard-link API. Windows uses the native
`CreateHardLinkW` API because Zig 0.16 does not implement Windows hard-link
creation in `std.Io`.

## Immutability

Hard links share the same underlying file and permissions. JMCL never modifies
a published object in place. Repairs and upgrades publish a new inode and
atomically replace instance links. Mutable files such as saves, logs, options,
configuration, and extracted natives never enter the shared store.

Automatic garbage collection is not yet implemented. Removing an instance
unlinks its paths without affecting the store or other instances.

The Windows validation checklist is available in
`docs/windows-hardlink-testing.md`.

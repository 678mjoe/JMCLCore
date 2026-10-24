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

1. Verify the instance destination.
2. Lock the object digest across processes, then verify the shared object under
   that lock.
3. Import an existing valid instance file when possible.
4. Otherwise download, hash, and atomically publish the object.
5. On Linux/macOS, create a temporary hard link in the instance and atomically
   replace the final destination. Windows retains a valid destination; an
   invalid destination is removed before creating its replacement link.

If hard links are unavailable, cross a filesystem boundary, or exceed the
filesystem link limit, JMCL copies the verified object instead. The copied
destination is rechecked by later installations, but does not have the atomic
replacement guarantee of the hard-link path. CLI and RPC results
report `hardLinked`/`files_hard_linked` and `copied`/`files_copied` counts.

Linux and macOS use the Zig directory hard-link API. Windows uses the native
`CreateHardLinkW` API because Zig 0.16 does not implement Windows hard-link
creation in `std.Io`.

## Immutability

Hard links share the same underlying file and permissions. JMCL never modifies
a published object in place. Repairs and upgrades publish a new inode and
replace instance links using the platform-specific steps above. Mutable files
such as saves, logs, options,
configuration, and extracted natives never enter the shared store.

## Garbage Collection

`store gc` reclaims cache objects that are no longer hard-linked into an
instance. It defaults to a dry run and a 24-hour minimum age:

```sh
jmcl-core store gc --store-directory .jmclcore
jmcl-core store gc --store-directory .jmclcore --min-age-seconds 3600 --execute
```

An object is eligible only when its canonical SHA-1 path is a regular file,
its hard-link count is exactly one, and it remains old enough. Dry-run is
read-only and reports an unlocked snapshot. `--execute` acquires the digest
lock and rechecks those conditions immediately before deletion. GC does not
scan instance manifests, hash object contents, or remove malformed paths and
temporary files. Copy-based instance files remain usable after collection, but
no longer retain their cache object.

Busy objects are skipped. Unexpected per-object failures increment `errors`;
the CLI exits nonzero and RPC returns `STORE_GC_PARTIAL` in that case.

Removing an instance unlinks its paths without affecting other instances. A
later GC run can reclaim the final remaining store link.

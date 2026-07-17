# Runtime Preparer

`install prepare` consumes an already downloaded and verified install cache.
It prepares launcher-managed native files and legacy asset layouts without
modifying the content-addressed source files.

```sh
jmcl-core install prepare 1.6.4 --directory ./.minecraft --os windows --arch x86_64
```

## Native Archives

All selected native JAR central directories are validated before the first
native output file is created. Validation rejects:

- Absolute, backslash, traversing, control-character, and unsafe Windows paths.
- Symlinks and other non-regular Unix archive entry types.
- Unsupported compression methods.
- More than 4,096 entries per archive.
- Entries larger than 256 MiB.
- More than 1 GiB expanded across the selected native set.
- Compression ratios above 1,000 to 1.
- Duplicate output files across archives.

Every archive is checked against its planned size and SHA-1 before preflight and
again before extraction. Metadata exclusions such as `META-INF/` are applied as
archive path prefixes.

Native files are extracted into a cryptographically random directory beneath:

```text
.jmcl/runtime/natives/<random-id>
```

The directory is deleted if extraction fails. On success, its relative path is
returned to the caller relative to `.minecraft` for `${natives_directory}`.
`launch.execute` resolves it
to an absolute path and removes it after the game process is fully reaped.

## Asset Projections

Legacy `virtual` and `map_to_resources` projections verify their source object
before publication. Existing valid destinations are reused. New destinations
prefer a hard link from the cache; unsupported or cross-device links fall back
to an atomic synchronized copy.

Projection destinations are never symlinks created by JMCL Core. Parent
directories are created as needed, and invalid existing files are atomically
replaced with verified content.

## Current Boundary

Preparation does not download missing files. A missing or invalid cache entry is
an error, so callers should run `install execute` first. Modern Minecraft
versions receive an empty native directory because their classified native
artifacts remain on the classpath and are not launcher-extracted.

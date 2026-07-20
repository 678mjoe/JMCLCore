# Modpack Installation

JMCL Core creates instances from modpack archives. Two formats are
recognized by the index file inside the zip: Modrinth packs carry
`modrinth.index.json` (`.mrpack`), CurseForge packs carry `manifest.json`.
A modpack install creates a new instance with the pack's Minecraft version
and mod loader, downloads every client-side content file through the same
verified content-addressed pipeline as single-content installs, registers
them in the content registry, and extracts the pack's overrides. Game files
are not downloaded; run `instance install` afterwards.

```sh
jmcl-core instance modpack install my-pack --file pack.mrpack
jmcl-core instance modpack install my-pack --file cf-pack.zip --api-key "$CF_KEY"
```

The target instance id must be free; an existing instance aborts the
install before anything is written. A failed install removes the partially
created instance.

## Modrinth Packs (`modrinth.index.json`)

- `formatVersion` must be 1 and `game` must be `minecraft`. The
  `dependencies` object pins the Minecraft version and at most one loader:
  `fabric-loader`, `forge`, or `neoforge`. Quilt packs and packs declaring
  multiple loaders are rejected.
- Every `files[]` entry is validated: safe relative path, SHA-1 presence,
  HTTPS-only download URLs, size caps, and `env` values. Files marked
  `client: "unsupported"` are server-only and skipped (counted in
  `files_skipped`).
- Files under `mods/`, `resourcepacks/`, and `shaderpacks/` are registered
  in the content registry with their kind. When the download URL is a
  Modrinth CDN URL, the project and version ids are parsed from it and the
  entry is registered with the `modrinth` provider; other URLs fall back to
  the `manual` provider. Files outside the three content directories are
  downloaded but not registered.
- Overrides are extracted from `overrides/` first and `client-overrides/`
  second, so client-specific files win.

## CurseForge Packs (`manifest.json`)

- `manifestType` must be `minecraftModpack` and `manifestVersion` 1. The
  primary `modLoaders[]` entry (`forge-47.2.0`, `fabric-0.16.10`,
  `neoforge-21.x`) selects the instance loader; other loaders are
  rejected.
- Every `files[]` entry references a numeric `projectID`/`fileID` pair and
  is resolved through the CurseForge API, which is why these packs require
  a caller-supplied `api_key` — the key is passed per request and never
  persisted, same as single-content installs. The project's class pins the
  content kind (`mc-mods` 6, resource packs 12, shader packs 6552);
  anything else is unsupported. Entries are registered with the
  `curseforge` provider.
- Files marked `"required": false` are opt-in and skipped.
- Overrides are extracted from the directory named by the manifest's
  `overrides` field (default `overrides`).

## RPC

```json
{"protocol":1,"id":"mp-1","method":"modpack.install","params":{"directory":"/path/to/instances","id":"my-pack","file":"/path/to/pack.mrpack"}}
```

`modpack.install` takes the instances root as `directory`, the new
instance id as `id`, the archive path as `file`, an optional `api_key`,
and the usual `store_directory`/`workers`/`retries`. Progress events match
the other download operations. The result reports the instance identity,
the selected loader (`null` for vanilla packs), `files_installed`,
`files_skipped`, and `overrides_written`. Stable error codes include
`INSTANCE_EXISTS`, `UNKNOWN_MODPACK_FORMAT`, `UNSUPPORTED_MODPACK_FORMAT`,
`UNSUPPORTED_LOADER`, `LOADER_CONFLICT`, `DOWNLOAD_UNAVAILABLE`, and
`MODPACK_NOT_FOUND`.

## Security Properties

- Archive entry names are validated as safe relative paths before any
  write (no traversal, no absolute paths, no Windows reserved names), and
  override extraction is bounded in count, per-file size, and total size.
- Every content file passes exact size and SHA-1 verification, store
  import, and hard-link materialization — identical to game artifacts.
- The content registry is written atomically and only after every download
  succeeded.
- The instance manifest is created before downloads begin; a failure at
  any later stage removes the whole instance directory.

## Excluded

- Modpack export, updates, and removal as a unit (instances are deleted
  with the normal `instance delete`).
- Quilt packs and CurseForge fingerprint matching.
- Optional-file selection (`required: false` files are skipped, not
  prompted for).
- Installing the game itself: `instance install` remains a separate step.

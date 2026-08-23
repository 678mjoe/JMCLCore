# Modrinth And CurseForge Content: Mods, Resource Packs, Shader Packs, And Data Packs

JMCL Core installs and manages Modrinth and CurseForge content for
instances. Mods target
instances with a mod loader (Fabric, NeoForge, or Forge); resource packs and
shader packs work on any instance, including vanilla. Content is
content-addressed like every other artifact: downloads are verified against
the Modrinth-published SHA-1, stored in the shared artifact store, and
hard-linked into the instance directory (`mods/` for mods, `resourcepacks/`
for resource packs, `shaderpacks/` for shader packs). Manually dropped files
can be adopted into the same scheme. Data packs follow the same pipeline but
attach to a single world (`saves/<world>/datapacks/`) with a world-scoped
registry; see below and `docs/worlds.md`.

This document covers installation and lifecycle management only. Searching and
browsing the Modrinth catalog is left to the embedding application.

## The Content Registry

Each instance records its managed content in `.minecraft/.jmcl/content.json`:

```json
{"schema_version":1,"entries":[
  {"kind":"mod","file":"mods/sodium-fabric-0.5.8+mc1.20.4.jar","sha1":"…","size":…,
   "source":{"provider":"modrinth","project_id":"AANobbMI","version_id":"…","slug":"sodium"},
   "enabled":true,"as_dependency":false},
  {"kind":"resourcepack","file":"resourcepacks/pack.zip","sha1":"…","size":…,
   "source":{"provider":"modrinth","project_id":"6SLU7tS5","version_id":"…","slug":"default-dark-mode"},
   "enabled":true,"as_dependency":false}
]}
```

- `kind` is `"mod"`, `"resourcepack"`, `"shaderpack"`, or `"datapack"`. The
  kind fixes the target directory (`mods/`, `resourcepacks/`, `shaderpacks/`,
  `datapacks/`) and the managed file extension (`.jar`, `.zip`); entries
  whose file does not live under their kind's directory are rejected.
- `enabled` marks active content. Disabling renames the instance file to
  `<file>.disabled` so the game no longer loads it; the registry keeps the
  canonical path, so project lookups, removal, and version switching work
  regardless of state. `.disabled` files are ignored by `adopt`.
- `source.provider` is `modrinth`, `curseforge`, or `manual`. Manually
  adopted files carry no project identifiers; CurseForge entries store the
  numeric mod id and file id as `project_id` and `version_id`.
- `as_dependency` marks content installed only to satisfy another project's
  required dependency.
- The registry is written atomically and only after every download succeeded;
  a failed install leaves no partial entries and no partial files.

`instance.json` remains the instance identity and is never modified by content
operations.

For data packs the registry root is the world directory, not the instance
game directory: `saves/<world>/.jmcl/content.json` (client) and
`servers/<id>/<level-name>/.jmcl/content.json` (server), with entries like
`datapacks/<file>`. Relative to that root the kind-directory rule holds
unchanged, so the whole pipeline is shared. The registry rides with the world
directory through rename, duplicate, and delete; world export/backup archives
exclude `.jmcl` to stay vanilla, so restored datapacks return as unmanaged
files that `adopt` re-registers.

## Mod Commands

```sh
jmcl-core instance mods install sodium-demo --project sodium
jmcl-core instance mods install sodium-demo --project sodium-extra --version M0ndiav7
jmcl-core instance mods install sodium-demo --project sodium --no-dependencies
jmcl-core instance mods list sodium-demo
jmcl-core instance mods list sodium-demo --json
jmcl-core instance mods adopt sodium-demo
jmcl-core instance mods set-version sodium-demo --project sodium --version FRXt5xaI
jmcl-core instance mods disable sodium-demo --project sodium
jmcl-core instance mods enable sodium-demo --project sodium
jmcl-core instance mods remove sodium-demo --project sodium-extra
jmcl-core instance mods remove sodium-demo --file mods/manual.jar
```

- `install` accepts a project slug or id and an optional exact Modrinth
  version id. Without `--version`, the newest compatible release wins over
  beta and alpha builds. Installing a project whose Modrinth type is not
  `mod` fails with a kind mismatch.
- The instance loader (`fabric_loader`, `neoforge_version`, or
  `forge_version` in the manifest) selects the Modrinth loader filter;
  instances without a loader reject mod installation.
- Required dependencies resolve recursively against the same Minecraft
  version and loader; already-registered projects are skipped and counted in
  the `skipped` line. Optional and incompatible dependencies are ignored.
  `--no-dependencies` installs only the requested project.
- `adopt` scans `mods/` for unregistered jars, hashes each into the artifact
  store, re-links the instance file as a hard link, and registers it with the
  `manual` provider.
- `set-version` switches a registered entry to an exact caller-selected
  version, upgrade or downgrade. The version must belong to the same project
  and support the instance's Minecraft version and loader; selecting the
  current version is a no-op. The new file downloads through the verified
  pipeline before the registry entry is updated atomically, and the old hard
  link is removed last. Dependencies are not touched, and manually adopted
  entries cannot be switched (they carry no provider identity). CurseForge
  entries take `--api-key` and a numeric file id. Switching the version of
  a disabled entry keeps the new file disabled on disk.
- `disable`/`enable` toggle a registered entry without deleting it, using
  the same `--project`/`--file` selectors as `remove`. Toggles are
  idempotent: repeating the current state is a no-op, and the rename follows
  the on-disk reality so a partial failure heals on retry.
- `remove` deletes the instance hard link and registry entry, including the
  `.disabled` variant of a disabled entry. The shared
  store object remains until `store gc` reclaims it after the last link
  disappears.

## Resource Pack Commands

```sh
jmcl-core instance resourcepacks install vanilla-demo --project default-dark-mode
jmcl-core instance resourcepacks list vanilla-demo
jmcl-core instance resourcepacks adopt vanilla-demo
jmcl-core instance resourcepacks remove vanilla-demo --project default-dark-mode
```

- Resource packs install into `resourcepacks/` and require no mod loader;
  vanilla instances are fully supported.
- Compatibility resolves through Modrinth's `minecraft` pseudo-loader:
  versions filtered by the instance's Minecraft version, newest stable release
  first. Installing a project whose Modrinth type is not `resourcepack` fails
  with a kind mismatch.
- Required dependencies recurse by default (same-type only; a resource pack
  that depends on a mod is rejected with a kind mismatch). `--no-dependencies`
  installs only the requested project.
- `list`, `adopt` (unmanaged `.zip` files), and `remove` behave exactly like
  their mod counterparts, scoped to the resource pack kind.
- Activating packs in `options.txt` is the embedding application's job; JMCL
  manages files only.

## Shader Pack Commands

```sh
jmcl-core instance shaderpacks install vanilla-demo --project bsl-shaders
jmcl-core instance shaderpacks list vanilla-demo
jmcl-core instance shaderpacks adopt vanilla-demo
jmcl-core instance shaderpacks remove vanilla-demo --project bsl-shaders
```

- Shader packs install into `shaderpacks/` and require no mod loader; vanilla
  instances are fully supported.
- Modrinth shader versions declare shader runtimes (`iris`, `optifine`, …)
  rather than the `minecraft` pseudo-loader, and JMCL does not track which
  shader runtime an instance runs. Compatibility therefore resolves by
  Minecraft version only, with no loader filter; newest stable release first.
  Installing a project whose Modrinth type is not `shader` fails with a kind
  mismatch.
- Required dependencies recurse by default (same-type only), as with the
  other kinds; `--no-dependencies` installs only the requested project.
- `list`, `adopt` (unmanaged `.zip` files), and `remove` behave exactly like
  their mod counterparts, scoped to the shader pack kind.
- Installing a shader runtime (Iris, Oculus, OptiFine) and activating packs
  is the embedding application's job; JMCL manages files only.

## Data Pack Commands

```sh
jmcl-core instance datapacks install vanilla-demo --world "My World" --project terralith
jmcl-core instance datapacks list vanilla-demo --world "My World"
jmcl-core instance datapacks adopt vanilla-demo --world "My World"
jmcl-core instance datapacks disable vanilla-demo --world "My World" --project terralith
jmcl-core instance datapacks remove vanilla-demo --world "My World" --project terralith
```

- Data packs attach to one world: every command takes `--world NAME` and
  operates on `saves/<world>/datapacks/` with the registry at
  `saves/<world>/.jmcl/content.json`. The instance's Minecraft version (from
  `instance.json`) drives compatibility.
- The managed form is the zip file, exactly like resource packs. Minecraft
  also accepts directory-shaped data packs in `datapacks/`; those are left
  alone and stay unmanaged (invisible to `list` and `adopt`).
- Modrinth types data packs as `mod` projects whose `loaders` list contains
  the `datapack` tag (the `project_type` field alone no longer distinguishes
  them). Datapack eligibility therefore accepts mod-typed projects tagged
  `datapack` — a multi-format pack like Terralith installs as a data pack
  through its zip versions and as a mod through its jar versions — and
  version selection filters by the `datapack` loader tag plus the
  Minecraft version, so only zip data pack files are ever picked. Installing
  a project without the datapack tag fails with a kind mismatch; explicit
  loader parameters are rejected.
- Mutations (`install`, `set-version`, `enable`, `disable`, `remove`,
  `adopt`) refuse a world held by a running game (`WORLD_LOCKED`, probed via
  `session.lock` exactly like world mutations) and require the world to
  exist (`WORLD_NOT_FOUND`). `list` never locks.
- `disable` renames to `<file>.disabled`; vanilla ignores a zip that no
  longer ends in `.zip`, so the pack stays dormant until re-enabled. Note
  that a pack whose author marked it required in `pack.mcmeta` may prevent
  the world from loading when disabled or removed — JMCL does not parse
  `pack.mcmeta`.
- Required dependencies recurse by default (same-type only), like the other
  kinds; `--no-dependencies` installs only the requested project.

## CurseForge

The same install path works against CurseForge by passing a provider and an
API key:

```sh
jmcl-core instance mods install jei-demo --project 238222 --provider curseforge --api-key "$CF_KEY"
```

- CurseForge has no slugs in its core API: `--project` takes the numeric
  mod id and `--version` the numeric file id. Resolving names to ids
  (search/browse) is the embedding application's job.
- The API key is **supplied by the caller** (the GUI), passed per request,
  and never persisted or echoed by the core — same discipline as the Entra
  client id. RPC methods take it as `api_key`; requests without one are
  rejected with `INVALID_PARAMS`.
- Kind mapping uses the CurseForge class: `mc-mods` (6), resource packs
  (12), shader packs (6552), data packs (5290). A class that does not match
  the requested kind fails with a kind mismatch; anything else is
  unsupported.
- Mod compatibility filters by game version and `modLoaderType` (Forge,
  Fabric, NeoForge); resource packs, shader packs, and data packs filter by
  game version only. The newest available stable file wins over beta and
  alpha.
- Files carry SHA-1 hashes and download through the same verified
  content-addressed pipeline. When an author disables third-party
  distribution (`downloadUrl` is null), the core asks the `download-url`
  endpoint; if that also yields nothing, the install fails with
  `DOWNLOAD_UNAVAILABLE`.
- Required dependencies (`relationType` 3) recurse by numeric mod id with
  the same bounds as Modrinth (depth 32, 256 projects, cycle-safe).
- `list`, `remove`, and `adopt` are provider-agnostic and unchanged;
  adopted files stay `manual` (CurseForge fingerprint matching is future
  work).

## RPC

The same operations are available over RPC as `mods.list`, `mods.install`,
`mods.set-version`, `mods.enable`, `mods.disable`, `mods.remove`,
`mods.adopt`, the matching `resourcepacks.*` and `shaderpacks.*` methods,
and the matching `datapacks.*` methods. The RPC forms take an explicit
`directory` and `minecraft_version` instead of reading an instance manifest;
`datapacks.*` additionally takes a required `world` and includes it in every
result. `mods.install` additionally requires `loader` (`fabric` | `neoforge`
| `forge`); `resourcepacks.install`, `shaderpacks.install`, and
`datapacks.install` take no loader.
All install methods accept `with_dependencies` (default `true`) to control
dependency recursion, plus `provider` (default `"modrinth"`) and `api_key`;
the CurseForge provider requires `api_key` and rejects any other use of it.
World datapack failures surface as `WORLD_NOT_FOUND` / `WORLD_LOCKED`
alongside the shared content error codes.

## Server Mods

`server.mods.*` applies the same registry and pipeline to a server instance:
params take the servers `directory` plus `id` instead of an instance game
directory, and the kind is always `mod` (dedicated servers never load
resource or shader packs). The Minecraft version and mod loader are derived
from `servers/<id>/server.json` instead of request parameters, so resolution
always matches the installed server; a vanilla server is rejected with
`SERVER_LOADER_REQUIRED`. The registry is `servers/<id>/.jmcl/content.json`
with the identical shape.

`server.mods.list` is read-only. Every mutating method (`install`,
`set-version`, `enable`, `disable`, `remove`, `adopt`) requires a stopped
server and returns `SERVER_ALREADY_RUNNING` otherwise, mirroring
`server.worlds.*` — mod changes take effect on the next start. Results,
progress events, provider rules (`api_key` for CurseForge, never persisted),
dependency recursion, and error codes match the client methods exactly.
Linux only, like every `server.*` method. The CLI mirrors the surface as
`jmcl-core server mods <list|install|set-version|enable|disable|remove|
adopt> ID ...`.

## Server Data Packs

`server.datapacks.*` manages `datapacks/` inside the server's base world
(`<level-name>/datapacks/`), with the registry at
`<level-name>/.jmcl/content.json`. There is no world parameter — datapacks
always target the world named by the `level-name` property, which must exist
and be usable (install the server first; otherwise
`SERVER_PROPERTIES_INVALID`). The `<level-name>` directory is created on
demand, so datapacks install before the first server start, and vanilla
servers are fully supported (no `SERVER_LOADER_REQUIRED` — data packs need
no loader). The Minecraft version comes from `server.json` like
`server.mods.*`; `server.datapacks.list` never locks and lists empty until
the world exists, while every mutation requires a stopped server
(`SERVER_ALREADY_RUNNING`) and refuses a `level-name` that is not a safe
single directory name. Provider rules, dependency recursion, progress
events, and error codes match the client `datapacks.*` methods; the CLI
mirrors the surface as `jmcl-core server datapacks <list|install|
set-version|enable|disable|remove|adopt> ID ...`. Linux only.

## Network And Mirrors

Modrinth API requests target `https://api.modrinth.com`; CurseForge API
requests target `https://api.curseforge.com` with the caller-supplied
`x-api-key` header. File downloads target the file URL published by each
API. None of them are rewritten by `source: "bmclapi"` — BMCLAPI mirrors
Minecraft content only.

The primary HTTP stack is the Zig standard library client. When its TLS
handshake fails, downloads and API calls fall back to a minimal HTTPS client
built on the vendored TLS stack in `src/vendor/`, which additionally supports
TLS 1.2 servers with ECDSA certificates (required by `cdn-alt.modrinth.com`).
The fallback preserves the same verification discipline: certificates are
checked against the operating system trust store, and every downloaded byte
still passes the exact size and SHA-1 checks of the install pipeline.

## Security Properties

- Project, version, and file metadata are validated: identifier character
  sets, HTTPS-only download URLs, SHA-1 presence, file size caps, and filename
  safety (no path separators, no Windows reserved names). Files are always
  placed at `<kind directory>/<validated filename>`, never at an API-supplied
  path.
- Project types are pinned to the requested kind: installing a mod through
  the resource pack commands (or vice versa) is rejected, as are cross-kind
  required dependencies. The CurseForge class id pins the kind the same way.
- Caller-supplied secrets (Microsoft OAuth material, the CurseForge API key)
  are validated before use and never persisted or echoed in results, events,
  or errors.
- Dependency resolution is bounded: depth 32, at most 256 projects per
  install, cycle-safe via a visited set.
- Every downloaded byte passes through the same verified pipeline as game
  artifacts: exact size and SHA-1 checks, atomic rename, store import, and
  hard-link materialization.
- Adopted files are hashed before import; the instance file is replaced by a
  hard link to the verified store object.

## Excluded

- Search, browse, and version listing RPCs (both providers).
- CurseForge modpacks and fingerprint-based adoption.
- Optional dependencies.
- Shader runtime selection and shader pack activation; `pack.mcmeta` and
  shader config format checks.
- Directory-shaped data packs as managed content (managed form is the zip);
  level.dat `DataPacks` enabled/disabled lists are never read or written;
  datapacks inside modpack overrides are not registered.

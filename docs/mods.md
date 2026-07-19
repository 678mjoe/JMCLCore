# Modrinth Mods

JMCL Core installs and manages Modrinth mods for instances that have a mod
loader (Fabric, NeoForge, or Forge). Mods are content-addressed like every
other artifact: downloads are verified against the Modrinth-published SHA-1,
stored in the shared artifact store, and hard-linked into the instance `mods/`
directory. Manually dropped jars can be adopted into the same scheme.

This document covers installation and lifecycle management only. Searching and
browsing the Modrinth catalog is left to the embedding application.

## The Content Registry

Each instance records its managed content in `.minecraft/.jmcl/content.json`:

```json
{"schema_version":1,"entries":[
  {"kind":"mod","file":"mods/sodium-fabric-0.5.8+mc1.20.4.jar","sha1":"…","size":…,
   "source":{"provider":"modrinth","project_id":"AANobbMI","version_id":"…","slug":"sodium"},
   "enabled":true,"as_dependency":false}
]}
```

- `kind` is currently always `"mod"`; the schema leaves room for
  `"resourcepack"` and `"shaderpack"` management later.
- `source.provider` is `modrinth` or `manual`. Manually adopted files carry no
  project identifiers.
- `as_dependency` marks mods installed only to satisfy another mod's required
  dependency.
- The registry is written atomically and only after every download succeeded;
  a failed install leaves no partial entries and no partial files.

`instance.json` remains the instance identity and is never modified by mod
operations.

## Commands

```sh
jmcl-core instance mods install sodium-demo --project sodium
jmcl-core instance mods install sodium-demo --project sodium-extra --version M0ndiav7
jmcl-core instance mods list sodium-demo
jmcl-core instance mods list sodium-demo --json
jmcl-core instance mods adopt sodium-demo
jmcl-core instance mods remove sodium-demo --project sodium-extra
jmcl-core instance mods remove sodium-demo --file mods/manual.jar
```

- `install` accepts a project slug or id and an optional exact Modrinth
  version id. Without `--version`, the newest compatible release wins over
  beta and alpha builds.
- The instance loader (`fabric_loader`, `neoforge_version`, or
  `forge_version` in the manifest) selects the Modrinth loader filter;
  instances without a loader reject mod installation.
- Required dependencies resolve recursively against the same Minecraft
  version and loader; already-registered projects are skipped and counted in
  the `skipped` line. Optional and incompatible dependencies are ignored.
- `adopt` scans `mods/` for unregistered jars, hashes each into the artifact
  store, re-links the instance file as a hard link, and registers it with the
  `manual` provider.
- `remove` deletes the instance hard link and registry entry. The shared
  store object remains until `store gc` reclaims it after the last link
  disappears.

The same operations are available over RPC as `mods.list`, `mods.install`,
`mods.remove`, and `mods.adopt`. The RPC forms take an explicit `directory`,
`minecraft_version`, and `loader` (`fabric` | `neoforge` | `forge`) instead of
reading an instance manifest.

## Network And Mirrors

Modrinth API requests target `https://api.modrinth.com`; file downloads target
the file URL published by the API (the Modrinth CDN). Neither is rewritten by
`source: "bmclapi"` — BMCLAPI mirrors Minecraft content only.

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
  placed at `mods/<validated filename>`, never at an API-supplied path.
- Dependency resolution is bounded: depth 32, at most 256 mods per install,
  cycle-safe via a visited set.
- Every downloaded byte passes through the same verified pipeline as game
  artifacts: exact size and SHA-1 checks, atomic rename, store import, and
  hard-link materialization.
- Adopted files are hashed before import; the instance file is replaced by a
  hard link to the verified store object.

## Excluded

- Search, browse, and version listing RPCs.
- CurseForge (requires a host-supplied API key; planned separately).
- Mod updates (`mods upgrade`), optional dependencies, and enable/disable
  toggles.
- Resource pack and shader pack management (registry schema already reserves
  their `kind` values).

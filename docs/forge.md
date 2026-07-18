# Forge

JMCL Core supports installing and launching an empty Forge client for modern
Minecraft versions (the installer spec shared with NeoForge, used by Forge
1.20.x). A Forge instance pins both its Minecraft version and its exact Forge
version. It does not include any third-party mods.

Forge cannot be combined with Fabric or NeoForge on the same instance. The
instance manifest records `forge_version` when selected; existing manifests
without that optional field are unaffected.

## Create An Instance

```sh
jmcl-core instance create forge-1.20.4 \
  --version 1.20.4 \
  --forge-version 49.2.8 \
  --name "Forge 1.20.4"
jmcl-core instance install forge-1.20.4
jmcl-core instance launch forge-1.20.4 --offline-name Player
```

Forge versions are Maven coordinates in the form `{minecraft}-{forge}`; for
example `49.2.8` targets Minecraft 1.20.4 as `1.20.4-49.2.8`. The core validates
that the downloaded installer matches both the requested Minecraft version and
the exact Forge version.

Direct install, prepare, plan, and launch commands accept the same
`--forge-version VERSION` option. RPC `install.plan`, `install.execute`,
`install.prepare`, `launch.plan`, and `launch.execute` accept an optional
`forge_version` string. `install.execute` additionally accepts optional
`java_paths` used to locate a Java runtime for the installer processors.

## How Installation Works

Installation follows the same two stages as NeoForge (see `docs/neoforge.md`),
with Forge-specific coordinates and verification:

1. **Download stage.** The core fetches
   `net/minecraftforge/forge/{minecraft}-{forge}/forge-{minecraft}-{forge}-installer.jar`
   from the official Forge Maven repository over HTTPS, extracts `version.json`
   and `install_profile.json` in memory, and merges `version.json` over the
   requested vanilla Minecraft version. All installer libraries carry published
   SHA-1 and byte sizes and enter the ordinary content-addressed install plan.
   The installer JAR itself is stored under
   `.jmcl/forge/{minecraft}-{forge}/installer.jar`.

2. **Processor stage.** The core runs the installer's client-side processors
   (`installertools` MCP_DATA/DOWNLOAD_MOJMAPS/MERGE_MAPPING,
   `ForgeAutoRenamingTool`, `binarypatcher`) with a locally detected Java
   runtime. The patched client is verified against the `PATCHED_SHA` checksum
   declared by the install profile before it is published into the
   content-addressed store by its computed SHA-1 and hard-linked back to
   `libraries/net/minecraftforge/forge/{minecraft}-{forge}/forge-{minecraft}-{forge}-client.jar`.
   `.jmcl/forge/{minecraft}-{forge}/state.json` records the result so
   reinstallation skips the chain when the patched client still matches.

The Forge `version.json` declares its patched client library with an empty
download URL; the core accepts that single entry only when its Maven coordinate
matches the install profile's `PATCHED` coordinate, excludes it from downloads,
and substitutes the locally patched JAR on the launch classpath instead.

Processor Java selection prefers the Java major version required by the
Minecraft version and falls back to any detected runtime with Java 8 or newer.
`instance install --java PATH` (and RPC `java_paths`) restricts the candidates.

`source: "bmclapi"` applies to Minecraft metadata and assets only. Forge
installer and Maven URLs remain their official HTTPS endpoints.

## Security Properties

All NeoForge processor security boundaries apply: capped profile sizes,
validated Maven coordinates, HTTPS-only URLs, verified processor classpaths,
shell-free `java -cp` invocation, captured output with byte limits, and no
server-side processors. In addition, the patched client output must match the
install profile's declared `PATCHED_SHA` when present; a mismatch fails the
install.

## Excluded

- Creating or downloading a `mods/` directory.
- Legacy Forge installers (1.12.2 and earlier), which use a different install
  profile layout.
- Modrinth, CurseForge, modpacks, Quilt, Fabric API, and third-party mods.

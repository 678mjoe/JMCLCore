# NeoForge

JMCL Core supports installing and launching an empty NeoForge client. A NeoForge
instance pins both its Minecraft version and its exact NeoForge version. It does
not include any third-party mods.

NeoForge cannot be combined with Fabric or Forge on the same instance. The
instance manifest records `neoforge_version` when selected; existing manifests
without that optional field are unaffected.

## Create An Instance

```sh
jmcl-core instance create neoforge-1.20.4 \
  --version 1.20.4 \
  --neoforge-version 20.4.251 \
  --name "NeoForge 1.20.4"
jmcl-core instance install neoforge-1.20.4
jmcl-core instance launch neoforge-1.20.4 --offline-name Player
```

The NeoForge version scheme derives from the Minecraft version: `20.4.x` targets
Minecraft 1.20.4, `21.1.x` targets 1.21.1. The core validates that the downloaded
installer matches the requested Minecraft version and NeoForge version exactly.

Direct install, prepare, plan, and launch commands accept the same
`--neoforge-version VERSION` option. RPC `install.plan`, `install.execute`,
`install.prepare`, `launch.plan`, and `launch.execute` accept an optional
`neoforge_version` string. `install.execute` additionally accepts optional
`java_paths` used to locate a Java runtime for the installer processors.

## How Installation Works

NeoForge does not publish a plain launcher profile. Its installer JAR contains a
`version.json` launcher profile, an `install_profile.json` processor chain, and
binary patches for the Minecraft client. Installation therefore has two stages:

1. **Download stage.** The core fetches
   `net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar` from the
   official NeoForged Maven repository over HTTPS, extracts both profiles in
   memory, and merges `version.json` over the requested vanilla Minecraft
   version. NeoForge libraries from the installer and from `version.json` enter
   the ordinary content-addressed install plan with their published SHA-1 and
   byte sizes and are verified like every other artifact. The installer JAR
   itself is stored under `.jmcl/neoforge/{version}/installer.jar`.

2. **Processor stage.** The installer requires the Minecraft client JAR to be
   disassembled, renamed, and patched locally. The core runs the installer's
   client-side processors (`installertools`, `jarsplitter`,
   `AutoRenamingTool`, `binarypatcher`) with a locally detected Java runtime:
   - Data placeholders such as `{MINECRAFT_JAR}`, `{MAPPINGS}`, and
     `[group:artifact:version:classifier@extension]` are resolved to absolute
     paths inside the instance directory; unknown placeholders fail the install.
   - Embedded data (for example `/data/client.lzma`) is extracted from the
     installer JAR; processor JARs and their classpaths come from the verified
     plan files, never from the network.
   - The patched client is published into the content-addressed store by its
     computed SHA-1 and hard-linked back to
     `libraries/net/neoforged/neoforge/{version}/neoforge-{version}-client.jar`.
     The launch classpath references the patched client instead of the vanilla
     client JAR, which remains installed as the patcher input.
   - `.jmcl/neoforge/{version}/state.json` records the patched SHA-1 and size.
     Reinstallation verifies the patched client against it and skips the
     processor chain when it still matches.

Processor Java selection prefers the Java major version required by the
Minecraft version and falls back to any detected runtime with Java 8 or newer.
`instance install --java PATH` (and RPC `java_paths`) restricts the candidates.

`source: "bmclapi"` applies to Minecraft metadata and assets only. NeoForge
installer and Maven URLs remain their official HTTPS endpoints.

## Security Properties

- Installer, version profile, and install profile sizes are capped; Maven
  coordinates, HTTPS URLs, SHA-1 values, and relative paths are validated before
  any download or file write.
- Library file names are always derived from validated Maven coordinates, never
  from path fields inside the installer.
- Processors run as direct `java -cp <verified jars> <main-class> <args>`
  processes without a shell. The main class is read from each processor JAR
  manifest and restricted to a safe character set. Processor output is captured
  with a byte limit and non-zero exits fail the install.
- No server-side processors are executed; entries marked `sides: ["server"]`
  are skipped.

## Excluded

- Creating or downloading a `mods/` directory.
- Legacy (pre-1.20.2) Forge installers, which use a different processor layout.
- Modrinth, CurseForge, modpacks, Quilt, Fabric API, and third-party mods.

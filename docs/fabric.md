# Fabric Loader

JMCL Core supports installing and launching an empty Fabric Loader client. A
Fabric instance pins both its Minecraft version and Fabric Loader version. It
does not include Fabric API or any third-party mods.

## Create An Instance

```sh
jmcl-core instance create fabric-1.21.1 \
  --version 1.21.1 \
  --fabric-loader 0.16.10 \
  --name "Fabric 1.21.1"
jmcl-core instance install fabric-1.21.1
jmcl-core instance launch fabric-1.21.1 --offline-name Player
```

The instance manifest records `fabric_loader` when selected. Existing manifests
without that optional field remain vanilla instances.

Direct install and launch commands accept the same `--fabric-loader VERSION`
option. RPC `install.plan`, `install.execute`, `install.prepare`,
`launch.plan`, and `launch.execute` accept an optional `fabric_loader` string.

## Resolution And Integrity

The core fetches the profile from Fabric's official metadata service for the
selected Minecraft and Loader versions. The profile must inherit exactly the
requested Minecraft version and use the expected profile ID.

Fabric libraries are normalized into the ordinary install plan and are placed
before inherited Minecraft libraries in the classpath. The Minecraft client JAR
remains the final classpath entry.

Every Fabric library must have a SHA-1 and byte size before installation. When
the official profile omits either value, the core obtains SHA-1 from the Maven
`.sha1` sidecar and size from an HTTPS `Content-Length` response. Missing,
invalid, zero-length, or oversized metadata fails planning. The existing
artifact store then verifies the exact hash and byte count during download.

`source: "bmclapi"` applies to Minecraft metadata and assets only. Fabric
profile and Maven URLs remain their official HTTPS endpoints.

## Excluded

- Creating or downloading a `mods/` directory.
- Installing Fabric API.
- Modrinth, CurseForge, modpacks, Quilt, Forge, and NeoForge.

# Instance Management

Managed instances isolate Minecraft versions and all mutable game content. The
default collection is `./instances`; every managed instance contains an atomic
manifest and its own single-directory Minecraft installation:

```text
instances/
  vanilla-26.1/
    instance.json
    .minecraft/
      assets/
      libraries/
      versions/
      logs/
      saves/
      resourcepacks/
```

Instance IDs use lowercase ASCII letters, digits, `.`, `_`, and `-`. They are
portable path components rather than display names. Names may contain spaces or
Unicode and are stored in the manifest.

## Create And Inspect

Creation verifies that the selected Minecraft version exists, then writes the
manifest atomically. It does not download game files.

```sh
jmcl-core instance create vanilla-26.1 \
  --version 26.1 \
  --name "Vanilla 26.1" \
  --source official

jmcl-core instance list
jmcl-core instance list --json
jmcl-core instance show vanilla-26.1
```

All commands accept `--instances-directory PATH`. The selected version and
download source are recorded in `instance.json`. Instance launch uses that
source. Installation uses it by default, but accepts an explicit one-time
`--source official|bmclapi` override without modifying the manifest.

## Install And Launch

```sh
jmcl-core instance install vanilla-26.1 --workers 8 --retries 2 \
  --store-directory .jmclcore
jmcl-core instance install vanilla-26.1 --source bmclapi
jmcl-core instance launch vanilla-26.1 --offline-name Player
```

Installation acquires immutable files through the shared `.jmclcore` artifact
store and hard-links them into the instance's `.minecraft`. Launch uses that
directory as the static data root and game working directory, while logs, saves,
options, resource packs, and future mutable mod configuration stay isolated.

When BMCLAPI is preferred, planning retries the complete metadata and asset
index pipeline through Mojang if the mirror is unavailable. The resulting file
plan still uses BMCLAPI URLs first and Mojang URLs as verified fallback.

An instance can additionally pin a Fabric Loader version in its optional
`fabric_loader` field. The field is set by `instance create --fabric-loader
VERSION`; installation and launch then resolve the official Fabric profile over
the instance's selected Minecraft version. Fabric API and third-party mods are
not installed automatically.

Instance launch currently supports the CLI offline identity and optional
`--arch` and repeatable `--java PATH` candidates. Microsoft authentication will
be added separately.

## Delete

Deletion validates both the instance ID and its manifest before recursively
removing the instance. The explicit `--force` flag prevents accidental deletion.

```sh
jmcl-core instance delete vanilla-26.1 --force
```

Directories without a valid matching `instance.json` are unmanaged and are not
deleted by this command.

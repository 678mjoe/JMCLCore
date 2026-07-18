# MVP Scope

## Goal

The first usable release launches an unmodified Minecraft: Java Edition
installation and exposes all long-running work through a stable JSON Lines
protocol suitable for a separate GUI process.

## Included

- Versioned JSON Lines RPC over standard input and standard output.
- Mojang version manifest and version metadata resolution.
- Installation of the client JAR, libraries, assets, and native libraries.
- Integrity checks for cached and downloaded files.
- Local Java discovery and Minecraft-to-Java compatibility checks.
- Microsoft device-code authentication and token refresh.
- A single selectable `.minecraft` runtime and game directory per instance.
- Version-pinned instance manifests and isolated install/launch commands.
- CLI-only offline identities for local launch testing.
- Launch plan generation and game process supervision.
- Structured progress, result, and error events.
- Windows, Linux, and macOS support.
- Empty Fabric Loader instances with pinned Loader versions.

## Excluded

- Third-party account systems.
- Forge, NeoForge, and Quilt.
- Modrinth, CurseForge, and modpack installation.
- Skin management and other social features.
- A graphical interface.

## Architecture boundaries

- `core` contains Minecraft behavior and does not depend on CLI argument parsing.
- `service` coordinates installations, instances, accounts, and launch tasks.
- `protocol` owns request parsing and response serialization.
- `platform` isolates operating-system-specific behavior.
- Standard output is protocol-only in RPC mode; diagnostics use standard error.
- Installation and launching first produce immutable plans, then execute them.
- Secrets are never included in logs or ordinary configuration files.

## Completion criteria

The MVP is complete when a GUI can log in, install a selected vanilla version,
launch it, display progress and logs, and receive a structured final result on
all three supported desktop platforms.

# JMCL Core

English | [简体中文](README-zh_CN.md)

JMCL Core is a Minecraft: Java Edition launcher core written in Zig. It is
designed to be driven by a separate graphical interface through a versioned
command-line protocol.

The project currently provides the protocol and CLI foundation, Mojang metadata
resolution, complete static installs, content-addressed hard-link storage,
concurrent verified downloads, secure native extraction, legacy asset projection,
Java runtime selection, launch planning, and supervised game execution.
Configurable Microsoft authentication and a credentialless public account
registry are available; support for
downloading and managing multiple Java versions is planned for a future
milestone.

## NOTICE

This is a vibe coding project.

## Note

JMCL Core provides a Minecraft: Java Edition launcher backend that anyone can
use. To create a launcher of your own, simply give your AI agent
`docs/gui-handoff.md`, a link to this repository, and a compiled `jmcl-core`
binary. As a Minecraft advancement reminds us: “Sky's the Limit.”

## Requirements

- Zig 0.16.0

## Build and test

```sh
zig build
zig build test
```

## CLI

```sh
zig build run -- version
zig build run -- version --json
zig build run -- version list --type release --limit 10
zig build run -- version list --type snapshot --limit 5 --json
zig build run -- version list --source bmclapi --limit 10
zig build run -- version show 1.21.11
zig build run -- version show 1.6.4 --json
zig build run -- version show 1.21.11 --source bmclapi
zig build run -- install plan 1.21.11
zig build run -- install plan 1.6.4 --os windows --arch x86_64 --json
zig build run -- install execute 1.21.11 --directory ./.minecraft --workers 8
zig build run -- install prepare 1.6.4 --directory ./.minecraft
zig build run -- store gc --store-directory ./.jmclcore
zig build run -- java detect
zig build run -- java select 26.2
zig build run -- launch execute 26.1 --directory ./.minecraft --offline-name Player
zig build run -- instance create vanilla-26.1 --version 26.1 --name "Vanilla 26.1"
zig build run -- instance create fabric-1.21.1 --version 1.21.1 --fabric-loader 0.16.10 --name "Fabric 1.21.1"
zig build run -- instance list
zig build run -- instance install vanilla-26.1
zig build run -- instance install vanilla-26.1 --source bmclapi
zig build run -- instance launch vanilla-26.1 --offline-name Player
zig build run -- rpc
```

The RPC mode reads one JSON request per line from standard input and writes one
JSON event per line to standard output. Logs and diagnostics must never be
written to standard output while RPC mode is active.

```json
{"protocol":1,"id":"request-1","method":"ping"}
{"protocol":1,"id":"request-2","method":"core.version"}
{"protocol":1,"id":"request-3","method":"version.list","params":{"type":"release","limit":10}}
{"protocol":1,"id":"request-4","method":"version.get","params":{"id":"1.21.11"}}
{"protocol":1,"id":"request-5","method":"version.get","params":{"id":"1.21.11","source":"bmclapi"}}
{"protocol":1,"id":"request-6","method":"install.plan","params":{"id":"1.21.11","source":"bmclapi","os":"windows","arch":"x86_64"}}
{"protocol":1,"id":"request-7","method":"install.execute","params":{"id":"1.21.11","directory":"./.minecraft","source":"bmclapi","workers":8}}
{"protocol":1,"id":"request-8","method":"install.prepare","params":{"id":"1.6.4","directory":"./.minecraft","os":"windows","arch":"x86_64"}}
{"protocol":1,"id":"request-9","method":"java.detect"}
{"protocol":1,"id":"request-10","method":"java.select","params":{"id":"26.2"}}
{"protocol":1,"id":"request-11","method":"launch.plan","params":{"id":"26.2","directory":"./.minecraft","natives_directory":".jmcl/runtime/natives/session-id","auth":{"player_name":"Player","uuid":"uuid","access_token":"secret"}}}
{"protocol":1,"id":"request-12","method":"launch.execute","params":{"id":"26.2","directory":"./.minecraft","auth":{"player_name":"Player","uuid":"uuid","access_token":"secret"}}}
```

Install, preparation, and launch commands default to `./.minecraft` when
`--directory` or the RPC `directory` field is omitted. Static runtime files,
native work directories, logs, options, saves, and resource packs all share
this single directory tree.

Managed instances live under `./instances` by default. Each instance pins one
Minecraft version and source in `instance.json`, and owns an isolated
`instances/ID/.minecraft` tree. Use `--instances-directory PATH` to select a
different instance collection.

An instance uses the source recorded in `instance.json` by default. Pass
`--source bmclapi` to `instance install` for a one-time mirror override.
BMCLAPI planning automatically retries through Mojang when mirror metadata is
unavailable, while static files remain BMCLAPI-first with official fallback.

Verified immutable files are stored once under `./.jmclcore/objects` and
materialized into `.minecraft` with hard links. Use `--store-directory PATH` to
select another store. Cross-filesystem placements automatically fall back to
atomic copies.

Mojang's official service is the default. Pass `--source bmclapi`, or use
`"source":"bmclapi"` in RPC parameters, to use BMCLAPI for users in mainland
China. BMCLAPI responses are still checked against Mojang-provided SHA-1 values.

See `docs/mvp.md` and `docs/protocol.md` for the current scope and protocol.
The install-plan format is documented in `docs/install-plan.md`.
Download execution is documented in `docs/installer.md`.
Runtime preparation is documented in `docs/preparer.md`.
Java selection is documented in `docs/java.md`.
Launch argument generation is documented in `docs/launch.md`.
Instance management is documented in `docs/instances.md`.
Shared artifact storage is documented in `docs/artifact-store.md`.
The content-addressed hard-link storage architecture is documented in
`docs/content-addressed-storage.md`.
Fabric Loader support is documented in `docs/fabric.md`.
NeoForge support is documented in `docs/neoforge.md`.
Forge support is documented in `docs/forge.md`.
Modrinth mod and resource pack management is documented in `docs/content.md`.
Microsoft authentication boundaries and the proposed RPC are documented in
`docs/authentication.md`.
Public account registry and credential ownership are documented in
`docs/accounts.md`.
Real approved-client authentication verification is documented in
`docs/authentication-verification.md`.

## License

JMCL Core is free software licensed under the GNU Lesser General Public License
version 3 or, at your option, any later version (`LGPL-3.0-or-later`). See
`LICENSE` and `COPYING` for the complete terms.

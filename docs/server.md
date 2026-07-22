# Server Management

Status: Phase 1 server instance storage and RPC CRUD are implemented. Phases
2–6 remain the roadmap. Multi-instance Minecraft server management is
**Linux first**—other platforms are explicitly deferred and every `server.*`
method returns `UNSUPPORTED_PLATFORM`. The GUI talks to a server-side
`jmcl-core` over SSH.

## Transport And Process Model

The GUI runs `ssh user@host jmcl-core rpc` and speaks the same JSON Lines
protocol as locally — no daemon, no HTTP layer, no new transport. The
synchronous session model and all existing methods work unchanged.

One architectural rule follows from SSH being the transport: **a running
game server must never be a child of an RPC session.** When the SSH
connection dies, its process group dies. Therefore:

- `server.start` detaches the server: `setsid`, console redirected to
  `logs/console.log`, stdin connected to a FIFO, pid recorded in the
  instance directory.
- All other lifecycle methods operate on on-disk state only (pid file,
  console log, FIFO). No RPC call holds a running server.

## Reuse Inventory

- Version metadata already carries the `server` download (URL + SHA-1) —
  it flows through the same verified content-addressed pipeline and
  artifact store as every other artifact.
- **Content-addressed storage with hard links remains the file-management
  invariant on servers too**: server jars, loader libraries, installer
  artifacts, and server mods are downloaded into the shared artifact store
  (size + SHA-1 verified), then hard-linked into each server instance via
  temp-name + atomic rename (copy fallback across devices). Multi-instance
  hosts keep exactly one copy of every shared artifact, and `store gc`
  reclaims objects whose last link disappears.
- `java.detect` / `java.select` pick the JVM.
- `launch/process.zig` provides supervised-process primitives.
- The content registry is directory-generic: pointing `mods.*` at a
  server instance's game directory manages server mods with zero new code.
- NeoForge/Forge installers natively support `--installServer`; the local
  processor-chain executors (`loaders/*_executor.zig`) are reusable.

## Instance Model

Server instances are separate from client instances:

```text
servers/<id>/server.json        identity: id, name, version_id, loader fields,
                                source, accept_eula, optional java_path
servers/<id>/server.jar         hard link into the artifact store (Phase 2)
servers/<id>/eula.txt           Phase 2
servers/<id>/server.properties Phase 2
servers/<id>/world/ logs/ mods/ config/   later phases

Mojang requires explicit EULA acceptance: install operations require
`"accept_eula": true` and record it; without it they fail with
`EULA_NOT_ACCEPTED`.

## Phases

### Phase 1 — Server instance store (implemented)

- `src/server/server_store.zig`: manifest CRUD with the same discipline as
  `instance_store.zig` (schema version, atomic writes, id validation).
- RPC `server.create | server.list | server.get | server.delete`.
- Platform gate: every `server.*` method returns `UNSUPPORTED_PLATFORM`
  off Linux.
- E2E: create/list/get/delete round-trip.

### Phase 2 — Vanilla installation and configuration

- `server.install`: resolve metadata → download the server jar through the
  verified pipeline → write `eula.txt` (given `accept_eula`) and a default
  `server.properties`. Select the JVM via `java.select`.
- `server.properties.get` / `server.properties.set` (key/value edits with
  atomic rewrite; port, motd, difficulty, etc.).
- E2E: install a vanilla server, set properties, verify on-disk results.

### Phase 3 — Process lifecycle

- `server.start`: refuse when already running; `setsid` detach; console to
  `logs/console.log` (append, bounded rotation); stdin FIFO at
  `.jmcl/console.fifo`; write `.jmcl/server.pid`.
- `server.stop`: write `stop` to the FIFO, wait a bounded grace period,
  escalate SIGTERM → SIGKILL; clean pid file.
- `server.restart`, `server.status` (pid alive + `/proc` cmdline
  verification, uptime), `server.logs` (tail with offset cursor),
  `server.command` (write one line to the FIFO).
- E2E on a Linux host/container: start → status → command → logs →
  restart → stop; kill -9 resilience (status detects stale pid file).

### Phase 4 — Modded servers

- Fabric: fabric-installer in server mode (or the fabric server launcher),
  producing `fabric-server-launch.jar`.
- NeoForge/Forge: installer with `--installServer`, reusing the local
  processor-chain executors; layout `libraries/`, `run.sh`,
  `user_jvm_args.txt`.
- Legacy Forge (1.7.10–1.12.2) server support evaluated separately.
- E2E: Fabric server with one mod (via `mods.install` pointed at the
  server directory) boots to `Done`.

### Phase 5 — Server content management

- Document and E2E the content-ops-on-server-directory pattern
  (`mods.list/install/set-version/enable/disable/remove/adopt` with
  `directory` = server instance root).
- Optional: `datapack` as a fourth content kind (`world/datapacks/*.zip`).

### Phase 6 — Hardening and polish

- World backups (`server.backup`: tar of `world*` through the artifact
  pipeline or plain archives).
- RCON/query support where enabled in `server.properties`.
- Revisit the Linux-only gate (macOS second, Windows last).

## Explicit Non-Goals (for now)

- No daemon/HTTP API: SSH + JSONL is the transport.
- No multi-node orchestration, metrics, or player management beyond what
  the console command channel provides.
- No client-instance changes: server instances are a parallel store.

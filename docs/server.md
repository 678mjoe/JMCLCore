# Server Management

Status: Phases 1–3 are implemented: server instance storage, vanilla
installation/properties, and detached process lifecycle. Phases 4–6 remain
the roadmap. Multi-instance Minecraft server management is
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
servers/<id>/server.jar         verified hard link into the artifact store
servers/<id>/.jmcl/install.json completed-install identity, server digest,
                                and required Java major version
servers/<id>/.jmcl/server.pid   validated detached-process state
servers/<id>/.jmcl/console.fifo bounded single-line console command channel
servers/<id>/logs/console.log   merged stdout/stderr with bounded rotation
servers/<id>/eula.txt           recorded EULA acceptance
servers/<id>/server.properties atomically managed key/value configuration
servers/<id>/world/ mods/ config/   later phases

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

### Phase 2 — Vanilla installation and configuration (implemented)

- `server.install` resolves Mojang metadata, selects a compatible JVM, and
  downloads `server.jar` through the shared size+SHA-1 verified artifact
  pipeline. The jar is materialized by hard link with atomic copy fallback.
- Every install request must pass `accept_eula:true`. The acceptance is stored
  in `server.json`, and successful installation atomically writes `eula.txt`.
- A missing `server.properties` receives conservative defaults. Existing
  properties are never overwritten by installation.
- `server.properties.get` returns the complete decoded key/value map.
  `server.properties.set` merges string-valued keys and atomically rewrites
  the file. Java properties escapes, continuations, Unicode escapes, and
  duplicate-key last-write semantics are supported.
- Successful installation writes `.jmcl/install.json`, including the required
  Java major used by lifecycle start. `server.get` and `server.list` report
  `installed:true` only when its version, loader, source, server file size,
  and current manifest still match. Missing, malformed, or pre-Phase-3
  markers return `false`; one idempotent reinstall upgrades the marker while
  reusing the verified cached jar.
- Phase 2 is vanilla-only. Loader-bearing manifests return
  `SERVER_LOADER_UNSUPPORTED` until Phase 4.
- Linux ARM64 E2E verified create → download Minecraft 1.21.4 → properties
  get/set → installed status → idempotent cache-hit reinstall.

### Phase 3 — Process lifecycle (implemented)

- `server.start` requires a matching completed installation, selects the
  recorded Java major version, refuses a live server, and launches through a
  short-lived `setsid` bootstrap. A detached supervisor is reparented outside
  the RPC session and owns the Java child until it exits; disconnecting SSH or
  ending `jmcl-core rpc` does not stop the server.
- `.jmcl/server.pid` atomically records the Java PID, supervisor PID, random
  launch identity, Java path, and wall-clock start time. Status and every
  mutating lifecycle operation validate both `/proc/<pid>/cmdline` and the
  random `-Djmcl.server.identity=...` JVM argument before trusting the PID.
  Missing, malformed, dead, or reused-PID state is treated as stopped and
  cleaned without signaling an unrelated process.
- Java stdout and stderr are merged into `logs/console.log`. The supervisor
  appends across restarts and rotates at 16 MiB, retaining `.1` and `.2`.
  `server.logs` tails by default or resumes from `cursor` plus `file_id`;
  replacement, rotation, or an out-of-range cursor resets safely to a tail.
- Java stdin is connected to `.jmcl/console.fifo`. `server.command` accepts
  one non-empty UTF-8 line up to 4095 bytes. `server.stop` sends `stop`, waits
  `grace_ms`, then escalates to SIGTERM and SIGKILL. `server.restart` performs
  the same bounded stop before launching a newly validated Java process.
- Lifecycle operations use an advisory per-server file lock so separate SSH
  RPC processes cannot race. Installation and deletion take the same lock and
  refuse a running server, preventing replacement or removal beneath a live
  Java process. The log pipes and console FIFO are OS channels; no RPC process
  owns or keeps an in-memory handle to a running server.
- Linux ARM64 container E2E verified RPC start → independent RPC exit →
  status → command → cursor-based logs → restart → graceful stop, plus a
  stubborn process that required SIGTERM/SIGKILL escalation.

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

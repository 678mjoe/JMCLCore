# Server Management

Status: Phases 1 through 5 are implemented for server instance storage,
vanilla installation/properties, detached process lifecycle, Fabric,
NeoForge, and modern Forge server installs, server mod management, and
server data pack management.
Legacy Forge server support was cut from Phase 4 and remains deferred.
Phase 6 remains the roadmap.
Multi-instance Minecraft server management is **Linux first**. Other platforms
are explicitly deferred and every `server.*` method returns
`UNSUPPORTED_PLATFORM`. The GUI talks to a server-side `jmcl-core` over SSH.

## Transport And Process Model

The GUI runs `ssh user@host jmcl-core rpc` and speaks the same JSON Lines
protocol as locally. There is no daemon, no HTTP layer, and no new transport. The
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

- Version metadata already carries the `server` download (URL + SHA-1). It
  flows through the same verified content-addressed pipeline and
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
- The content registry is directory-generic: content operations point at a
  directory, so `server.mods.*` reuses the client pipeline unchanged against
  the server instance root.
- NeoForge and modern Forge expose installer profiles that the local
  processor-chain executors can run with `SIDE=server`. JMCL Core executes
  those processor steps through its verified download pipeline instead of
  launching the installers as server setup tools.

## Instance Model

Server instances are separate from client instances:

```text
servers/<id>/server.json        identity: id, name, version_id, loader fields,
                                source, accept_eula, optional java_path
servers/<id>/server.jar         verified hard link into the artifact store
servers/<id>/libraries/         loader libraries and generated loader files
servers/<id>/.jmcl/install.json completed-install identity, server digest,
                                required Java major, and launch descriptor
servers/<id>/.jmcl/neoforge-server/  NeoForge server-side processor support
servers/<id>/.jmcl/forge-server/     modern Forge server-side processor support
servers/<id>/.jmcl/fabric_args.txt   generated Fabric server args file
servers/<id>/.jmcl/server.pid   validated detached-process state
servers/<id>/.jmcl/console.fifo bounded single-line console command channel
servers/<id>/logs/console.log   merged stdout/stderr with bounded rotation
servers/<id>/eula.txt           recorded EULA acceptance
servers/<id>/server.properties atomically managed key/value configuration
servers/<id>/mods/                registered server mods (registry below)
servers/<id>/.jmcl/content.json   server content registry (same shape as
                                  client instances)
servers/<id>/world/ config/       mutable server-private state

Mojang requires explicit EULA acceptance: install operations require
`"accept_eula": true` and record it; without it they fail with
`EULA_NOT_ACCEPTED`.

## Phases

### Phase 1: Server instance store (implemented)

- `src/server/server_store.zig`: manifest CRUD with the same discipline as
  `instance_store.zig` (schema version, atomic writes, id validation).
- RPC `server.create | server.list | server.get | server.delete`.
- Platform gate: every `server.*` method returns `UNSUPPORTED_PLATFORM`
  off Linux.

### Phase 2: Vanilla installation and configuration (implemented)

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
  Java major used by lifecycle start and a launch descriptor with
  `{mode,file,sha1,size}`. Marker schema v3 is current. v2 markers report
  `installed:false`; one idempotent reinstall upgrades the marker while
  reusing verified cached artifacts.
- `server.get` and `server.list` report `installed:true` only when version,
  loader, source, `server.jar` size, launch file size, and current manifest
  still match. The installed check does not re-hash files at start time and
  does not prove every loader library is still present. If `libraries/` is
  wiped while `server.jar` and the launch file remain, reinstall is the repair
  path.

### Phase 3: Process lifecycle (implemented)

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

### Phase 4: Modded servers (implemented for Fabric, NeoForge, modern Forge)

- Vanilla remains the simplest install shape: `server.jar` is downloaded
  through the verified artifact pipeline, the launch descriptor uses
  `mode:"jar"`, and lifecycle starts Java with `-jar server.jar nogui`.
- NeoForge servers materialize loader libraries under root `libraries/` and
  keep server-side processor support under `.jmcl/neoforge-server/<version>/`.
  The local processor chain runs with `SIDE=server` and `MINECRAFT_JAR` set to
  `server.jar`. The generated launch file is the loader's
  `libraries/net/neoforged/neoforge/<version>/unix_args.txt`, recorded in the
  launch descriptor as `mode:"args_file"`.
- Modern Forge servers use the same verified processor-chain model with
  `SIDE=server`, root `libraries/`, and support files under
  `.jmcl/forge-server/<minecraft>-<forge>/`. The generated launch file is
  `libraries/net/minecraftforge/forge/<minecraft>-<forge>/unix_args.txt`, also
  recorded as `mode:"args_file"`.
- NeoForge and modern Forge may leave `run.sh` and `user_jvm_args.txt` on disk
  because the loader processor chain creates them. The supervisor does not use
  them as launch surfaces. It reads `.jmcl/install.json` and launches Java with
  the recorded args file.
- Fabric servers are resolved from Fabric Meta `server/json`. Libraries are
  verified through provided hashes or maven `.sha1` sidecars, then written
  under root `libraries/`. JMCL Core generates `.jmcl/fabric_args.txt` with a
  classpath ending in `server.jar` and the `KnotServer` main class, then
  launches with `mode:"args_file"`.
- The launch descriptor is exactly `{mode,file,sha1,size}`. `mode` is either
  `jar` or `args_file`; `file` is a relative path inside the server directory;
  `sha1` and `size` are recorded from the verified launch file at install time.
- BMCLAPI affects Mojang metadata and the Mojang `server.jar` only. Loader
  maven URLs for NeoForge, Forge, and Fabric stay direct so their published
  hashes and maven sidecars remain the source of truth.
- NeoForge and modern Forge processors execute third-party loader Java locally,
  with the same trust posture as client loader installs. Processor downloads
  still flow through the verified pipeline when JMCL Core planned them, but the
  loader processor step named `DOWNLOAD_MOJMAPS` may contact Mojang mappings
  from inside the processor process.
- Legacy Forge server support is deferred by the Phase 4 cut condition. Legacy
  Forge manifests continue to fail server install planning with
  `SERVER_LOADER_UNSUPPORTED`; no legacy server directory layout is documented
  as implemented.

### Phase 5: Server content management (implemented for mods)

- `server.mods.list | install | set-version | enable | disable | remove |
  adopt` apply the content-ops-on-server-directory pattern with
  `directory` = servers root plus `id`. The registry lives at
  `servers/<id>/.jmcl/content.json` with the exact client shape, and files
  download through the same verified content-addressed pipeline.
- The game version and mod loader are derived from `server.json`, never
  supplied by the caller, so resolutions always match the installed
  server. Vanilla servers are rejected with `SERVER_LOADER_REQUIRED`.
- Mod installs support both providers (`provider`, caller-supplied
  `api_key` for CurseForge) and dependency recursion with the same bounds
  and opt-out as client installs.
- `list` is read-only; every mutating method refuses a running server with
  `SERVER_ALREADY_RUNNING` through the same lifecycle lock as worlds, so
  mod changes take effect on the next start.
- CLI parity: `jmcl-core server mods <list|install|set-version|enable|
  disable|remove|adopt> ID ...`.

### Phase 5b: Server data packs (implemented)

- `server.datapacks.list | install | set-version | enable | disable |
  remove | adopt` manage `<level-name>/datapacks/` with the registry at
  `<level-name>/.jmcl/content.json` — the same world-scoped design as client
  `datapacks.*` (the deferral note about kind-directory invariance was
  resolved by scoping the registry to the world directory itself).
- There is no world parameter: the base world comes from the `level-name`
  property, which must exist and be a safe single directory name
  (`SERVER_PROPERTIES_INVALID` otherwise — install the server first). The
  world directory is created on demand, so datapacks install before the
  first start; `list` lists empty until then.
- Vanilla servers are fully supported (no `SERVER_LOADER_REQUIRED` — data
  packs need no loader); the game version is derived from `server.json`.
- `list` is read-only; every mutation requires a stopped server
  (`SERVER_ALREADY_RUNNING`) through the same lifecycle lock as mods and
  worlds. Provider rules (`api_key` for CurseForge), dependency recursion,
  managed zip-only form, and `.disabled` toggling match the client datapack
  semantics in `docs/content.md`.
- CLI parity: `jmcl-core server datapacks <list|install|set-version|
  enable|disable|remove|adopt> ID ...`.

### Phase 6: Hardening and polish

- World backups are already implemented (`server.worlds.backup` and
  friends); remaining candidates here are RCON/query support where enabled
  in `server.properties`.
- Revisit the Linux-only gate (macOS second, Windows last).

## Explicit Non-Goals (for now)

- No daemon/HTTP API: SSH + JSONL is the transport.
- No multi-node orchestration, metrics, or player management beyond what
  the console command channel provides.
- No client-instance changes: server instances are a parallel store.

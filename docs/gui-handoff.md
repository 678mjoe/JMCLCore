# JMCL Core — GUI Integration Contract

Audience: the agent/developer building the JMCL GUI in a separate repository.
This document is self-contained; reading the core's source is not required.
Details beyond the contract live in `docs/*.md` of the core repository
(`protocol.md`, `content.md`, `modpack.md`, `authentication.md`, `accounts.md`,
`java.md`, `launch.md`, `worlds.md`).

JMCL Core is a Minecraft: Java Edition launcher core distributed as a single
native binary (`jmcl-core`). The GUI drives it through a JSON Lines RPC
session. The core owns all filesystem, network, download-verification, and
process-supervision work. The GUI owns presentation, catalog search/browse,
credential storage, and all caller-supplied secrets.


## Contract baseline and document ownership

This contract describes the shipping **Zig Core** source at
[`5a4310debbcee26b3d2391bab2339f79cd6ec52e`](https://github.com/678mjoe/JMCLCore/commit/5a4310debbcee26b3d2391bab2339f79cd6ec52e).
A later documentation-only commit does not change the frontend's pinned binary.
The Core copy is `docs/gui-handoff.md`; the frontend mirror is `gui-handoff.md`.
Keep the shared contract text aligned and verify changes against source, not just
one document against the other. UI implementation is **not** implied by a method
appearing here. See [frontend capability status and implementation plan](https://github.com/678mjoe/JMCL-frontend/blob/main/docs/frontend-core-capability-report.md).

The independent `rust-rewrite` branch is not the packaged sidecar or a finished
replacement. New Core features are queued for Rust after rewrite readiness;
Zig main is not the feature-development priority. Cancellation semantics still
require a separate design decision. Future proposals are not v1 guarantees.

## 1. Process Model

- Start one child process: `jmcl-core rpc`. It is a **persistent session**:
  requests on stdin, events on stdout (flushed per event), diagnostics on
  stderr. Closing stdin ends the session cleanly.
- **Synchronous, one request at a time per session** (protocol v1). A request
  emits zero or more progress events followed by exactly one terminal `result`
  or `error` event. Do not send the next request until the terminal event
  arrives.
- For concurrency (e.g. installing an instance while browsing content of
  another), spawn **multiple sessions**. Operations on *different* instance
  directories are independent; the shared artifact store is lock-protected.
  Never run two mutating operations against the *same* instance directory
  concurrently.
- Protocol v1 has no cancellation. Long operations (`install.execute`,
  `launch.execute`, `modpack.install`) run to completion; killing the child is
  the only abort, not a transaction rollback. Treat an interrupted operation
  as incomplete: inspect its state and reverify/repair install artifacts before
  use. The shared store verifies artifacts before reuse, but cross-filesystem
  copy fallback and Windows destination replacement are not atomic (see
  [storage boundaries](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/docs/content-addressed-storage.md#atomic-materialization)).
  Do not blindly replay mutable world operations. Treat any in-flight
  credential rotation as needing recovery (see §6).

### File-management invariant

Hard-link-based sharing of verified immutable artifacts is a defining design
constraint: multiple isolated instances may reference the same store object.
Keep one shared store where possible; a filesystem that cannot hard-link may
require copy fallback. Never edit a shared object in place to customize one
instance. Mutable worlds, logs, options and configuration remain instance-private.
Deleting an instance link is not permission to delete a still-shared store object;
use Core's lifecycle and GC interfaces, not GUI-side cache cleanup guesses.

## 2. Wire Format

Request — one line, UTF-8 JSON, ≤ 64 KiB:

```json
{"protocol":1,"id":"req-42","method":"ping","params":{}}
```

- `protocol` must be `1`. Mismatch → `UNSUPPORTED_PROTOCOL` (check at startup
  and surface a core/GUI version incompatibility).
- `id` is a non-empty client-chosen string, echoed in every event of that
  request. Unknown request fields are ignored (forward-compatible).

Events:

```json
{"protocol":1,"id":"req-42","event":"result","result":{"pong":true}}
{"protocol":1,"id":"req-42","event":"error","error":{"code":"METHOD_NOT_FOUND","message":"Unknown RPC method"}}
```

- Malformed requests get an error with `id: null`; the session survives.
- Download-style operations emit progress before the terminal event:

```json
{"protocol":1,"id":"i-1","event":"started","source":"official","started":{"stage":"download","version_id":"1.21.4","files_total":320,"bytes_total":52428800}}
{"protocol":1,"id":"i-1","event":"file","source":"official","file":{"path":"mods/sodium.jar","status":"downloaded","placement":"hard_link","size":1306799,"attempts":1}}
{"protocol":1,"id":"i-1","event":"retry","source":"official","retry":{"path":"assets/objects/ab/abcd","attempt":1,"max_attempts":3,"error":"UnsuccessfulHttpStatus"}}
{"protocol":1,"id":"i-1","event":"progress","source":"official","progress":{"files_completed":12,"files_total":320,"bytes_verified":4194304,"bytes_processed":4456448,"bytes_total":52428800,"bytes_transferred":2097152}}
{"protocol":1,"id":"i-1","event":"processor_started","source":"official","processor_started":{"stage":"processor","index":2,"total":4,"name":"net.neoforged:installertools:2.1.3"}}
```

For the main progress bar use `bytes_processed / bytes_total`.
`bytes_processed` advances during response streaming (256 KiB byte threshold,
globally throttled to at most one event per 100 ms), is monotonic across
parallel workers and retries, and never exceeds `bytes_total`.
`bytes_verified` advances only after full hash verification and
materialization; `bytes_transferred` counts successful network downloads.
Cache hits jump `bytes_processed` when verification completes.

- `launch.execute` additionally emits `started` (with `pid` + redacted argv),
  `stdout`/`stderr` (`{"sequence":N,"encoding":"base64","end":"newline|more","data":"…"}`),
  and a terminal `result` whose `process` object reports `termination`
  (`exited|signaled|stopped|unknown`), exit code/signal, duration, and max RSS.
  When Java selection downloads a managed runtime before spawning, a `runtime`
  event carries the same stage/file/retry/progress payloads as
  `java.runtime.install`.
- Many results carry a top-level `source` field (`official`|`bmclapi`). If
  your UI lets users pick BMCLAPI, you **must visibly attribute BMCLAPI**
  (its service policy: <https://bmclapidoc.bangbang93.com/>).

## 3. Startup Handshake

```json
→ {"protocol":1,"id":"0","method":"core.version"}
← {"protocol":1,"id":"0","event":"result","result":{"name":"jmcl-core","version":"0.1.0-dev","protocol":1}}
```

Verify `protocol == 1` before anything else. `ping` checks liveness.

## 4. Method Reference

### Versions and game installation

| Method | Required params | Optional params | Result highlights |
|---|---|---|---|
| `version.list` | — | `type`, `limit` (1–1000, default 20), `source` | `latest`, `versions[]` |
| `version.get` | `id` | `source` | full normalized version metadata |
| `version.resolve` | `id` | one loader field, `source` | merged `{kind, id, java_major_version}`; the standalone form of the `instance.create` validation step; `METADATA_TIMEOUT` on fetch expiry |
| `install.plan` | `id` | `source`, `os`, `arch`, loader fields | deduped file list, classpath, natives, total size |
| `install.execute` | `id` | `directory` (default `.minecraft`), `store_directory` (default `.jmclcore`), `source`, `os`, `arch`, loader fields, `workers` (1–32, default 8), `retries` (0–5, default 2) | started/file/retry/progress events, processor_started for NeoForge/Forge, final counters |
| `install.prepare` | `id` | `directory`, `source`, `os`, `arch`, loader fields | `native_directory` (pass to `launch.plan`) |

Loader fields: at most one of `fabric_loader`, `neoforge_version`,
`forge_version` per request. NeoForge/Forge `install.execute` runs the
official processor chain locally and emits `processor_started` with the
CLI-equivalent `index`, `total`, and processor `name`; its result reports
`processors_run` and the `patched_client` SHA-1. Typical full install:
`install.execute` →
`install.prepare` → `launch.execute`.

### Java

| Method | Required params | Optional params | Result highlights |
|---|---|---|---|
| `java.detect` | — | `paths[]` | probed runtimes + per-candidate failures |
| `java.select` | `id` | `source`, `arch`, `paths[]` | compatible runtime (exact major + arch) |
| `java.runtime.list` | — | `store_directory` | managed runtimes with provider, component/package, probed version, executable |
| `java.runtime.install` | `component` or `major` (exactly one) | `provider` (`auto`\|`mojang`\|`zulu`), `platform`, `store_directory`, `workers`, `retries` | stage/file/retry/progress events, runtime receipt; `JAVA_DOWNLOAD_FAILED` on download failure |
| `java.runtime.remove` | `runtime` | `store_directory` | removes the managed runtime tree |

`launch.plan`, `launch.execute`, and `install.execute` accept
`java_policy` (`auto` default, `local`, `managed`), `java_paths[]` (candidate
executables), and `store_directory`;
the launch methods also accept `java_override` (explicit executable, skips
the exact-major/architecture gates, mutually exclusive with non-empty `java_paths`).
A forced override is not an ordinary selection preference: it bypasses the
major/architecture gates and does not supply Java to loader processors.
`local` never uses/downloads managed runtimes; `managed` prefers an installed
managed runtime, then compatible local Java, then a managed download. It does
**not** mean managed-only. Candidate probing executes the chosen program; ask
users to select only trusted Java executables. Preferences must be scoped by
instances root + instance ID, not the ID alone.
Default `auto` covers most cases: local exact-major first, then installed
managed runtimes, then a managed download. Pre-provision with
`java.runtime.install` when the GUI wants explicit control (for example
downloading Java during first-run setup); `launch.plan` never downloads and
reports `JAVA_INCOMPATIBLE` when nothing installed matches.

### Launch

| Method | Required params | Notes |
|---|---|---|
| `launch.plan` | `id`, `natives_directory`, `auth` | diagnostic; returns `redacted_argv`, never raw tokens |
| `launch.execute` | `id`, `auth` | owns natives internally; emits process events (§2) |

`auth` session object: `{"player_name":"…","uuid":"…","access_token":"…"}` with
optional `"client_id"`/`"xuid"`. `options` may include
`resolution_width`/`resolution_height`.

**Offline sessions** are supported: construct the session client-side as
`player_name` = chosen name, `uuid` = hex MD5 of `"OfflinePlayer:" ++ name`
(no dashes, matching the vanilla offline-mode derivation),
`access_token` = `"offline"`, `"user_type":"legacy"`. The CLI twin is
`--offline-name`.

### Instances and content

Instances are managed over RPC. `directory` is the instances root; an
instance is `instances/<id>/` with `instance.json` plus the game directory
`instances/<id>/.minecraft` that all content methods operate on:

| Method | Required params | Optional params | Notes |
|---|---|---|---|
| `instance.create` | `directory`, `id`, `version_id` | `name` (defaults to id), one loader field, `source`, `validate` (default true) | resolves the version+loader online first; returns `installed:false`; `INSTANCE_EXISTS` if taken; `METADATA_TIMEOUT` when resolution stalls |
| `instance.list` | `directory` | — | `{instances[]}` sorted by id; each includes `installed` |
| `instance.get` | `directory`, `id` | — | one manifest with `installed`; `INSTANCE_NOT_FOUND` |
| `instance.delete` | `directory`, `id` | — | removes the whole instance directory |

```json
{"protocol":1,"id":"ic-1","method":"instance.create","params":{"directory":"/path/to/instances","id":"my-instance","name":"My Instance","version_id":"1.21.4","fabric_loader":"0.16.10"}}
```

A manifest looks like:

```json
{"id":"my-instance","name":"My Instance","version_id":"1.21.4","fabric_loader":"0.16.10","neoforge_version":null,"forge_version":null,"source":"official","installed":false}
```

For a responsive create flow, pass `"validate":false` to write the manifest
immediately, then call `version.resolve` on a separate session to check the
combination in the background and offer `instance.delete` when it fails.
Metadata fetches are bounded by a 30-second timeout and served from the
on-disk metadata cache when fresh (see `docs/protocol.md`), so repeated
resolves of the same combination are cheap.

`installed` becomes true only after `install.execute` fully succeeds for the
manifest's exact Minecraft version and loader. Download source does not affect
the status because artifacts are SHA-1 verified. A missing, corrupt, or
version/loader-mismatched completion marker returns false. Loader instances
installed by affected older core builds may return false once; reinstalling is
safe because verified artifacts are reused through the shared store and hard
links.

`id` rules: ≤64 chars, starts `[a-z0-9]`, then `[a-z0-9._-]`, no trailing dot,
no Windows reserved stems. At most one loader field (`fabric_loader`,
`neoforge_version`, `forge_version`).

World (save) management operates on the same game `directory` as content
methods. Worlds are mutable instance-private state: never content-addressed,
never hard-linked. Mutations use staging and probe for worlds in use by a
running game (`session.lock` held -> `WORLD_LOCKED`; stale lock files are
tolerated). The probe does not prevent an external launch after the check;
callers still coordinate launches and mutations. See [world mutation and
interruption boundaries](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/docs/worlds.md#mutations):

| Method | Required params | Optional params | Notes |
|---|---|---|---|
| `worlds.list` | `directory` | `minecraft_version` | `{worlds[]}` sorted by name; corrupt worlds list with `null` metadata |
| `worlds.get` | `directory`, `world` | `minecraft_version` | one `world` entry plus `icon_png_base64` (null when absent) |
| `worlds.rename` | `directory`, `world`, `new_name` | `minecraft_version` | renames the directory AND rewrites `LevelName` inside `level.dat` atomically; returns the renamed entry |
| `worlds.duplicate` | `directory`, `world`, `new_name` | `minecraft_version` | plain recursive copy (no links, `session.lock` excluded); returns the new entry |
| `worlds.delete` | `directory`, `world` | — | permanent; confirm in the GUI first |
| `worlds.export` | `directory`, `world`, `file` | — | writes a standard zip; returns `{file, size_bytes, files}`; refuses locked worlds |
| `worlds.import` | `directory`, `file` | `name`, `replace` | extracts a world zip; default name from the zip prefix or file stem; returns the world entry |
| `worlds.backup` | `directory`, `world` | `label` | timestamped zip into `<game dir>/backups/`; returns `{backup}` |
| `worlds.backups` | `directory` | — | `{backups[]}` newest first: `file`, `world` (nullable), `created_ms`, `size_bytes` |
| `worlds.restore` | `directory`, `backup` | `name`, `replace` | import from the backups directory; `WORLD_EXISTS` unless `replace:true` (full swap) |
| `worlds.backups.delete` | `directory`, `backup` | — | `BACKUP_NOT_FOUND` for missing files |

World entries: `name` (directory), parsed `level.dat` metadata
(`level_name`, `version_name`, `version_id`, `game_mode`, `hardcore`,
`cheats`, `difficulty`, `last_played_ms` — all nullable), `size_bytes`,
`has_icon`, `locked`, and `version_relation` (`same`/`different`/`unknown`
against your `minecraft_version`; advisory only — surface a downgrade warning
yourself, the core never blocks). `world` names: 1-255 bytes, no separators,
no leading dot, no trailing space/dot. See `docs/worlds.md`.

Server instances use a separate root and are Linux-only. A remote GUI starts
`ssh user@host jmcl-core rpc` and uses the same JSON Lines protocol. Every
`server.*` call returns `UNSUPPORTED_PLATFORM` when the core is not running
on Linux.

#### Server request convention

`directory` is the servers root and `id` identifies one directory below it.
The GUI passes both values to the core and must not construct internal
`.jmcl`, world, log, mod, or backup paths itself:

```json
{"protocol":1,"id":"srv-1","method":"server.status","params":{"directory":"/srv/jmcl/servers","id":"demo"}}
```

Server calls use the normal terminal-event contract. A success is one
`event:"result"`; a failure is one `event:"error"`. Messages are for display;
branch on the stable error `code`. Unknown request fields are ignored for
forward compatibility, but the GUI should send only documented fields.

#### Server method inventory

| Method | Required params | Optional params | Notes |
|---|---|---|---|
| `server.create` | `directory`, `id`, `version_id` | `name`, one loader field, `source`, `accept_eula` (default false), `java_path` | resolves version+loader online; writes `servers/<id>/server.json`; `SERVER_EXISTS` if taken |
| `server.list` | `directory` | — | `{servers[]}` sorted by id |
| `server.get` | `directory`, `id` | — | one server manifest; `SERVER_NOT_FOUND` |
| `server.delete` | `directory`, `id` | — | removes the whole stopped server instance directory; `SERVER_ALREADY_RUNNING` while live |
| `server.install` | `directory`, `id`, `accept_eula:true` | `store_directory`, `workers`, `retries`, `java_paths[]` | stopped servers only; installs vanilla, NeoForge, modern Forge, or Fabric with compatible Java selection; writes `server.jar`, loader launch files when needed, `eula.txt`, defaults, and completion state |
| `server.start` | `directory`, `id` | `java_paths[]` | requires a matching completed install; returns detached Java/supervisor PIDs, start time, and selected Java |
| `server.stop` | `directory`, `id` | `grace_ms` (default 10000, max 120000) | Minecraft `stop`, then bounded SIGTERM/SIGKILL escalation; returns `termination` |
| `server.restart` | `directory`, `id` | `java_paths[]`, `grace_ms` | bounded stop followed by a fresh detached start |
| `server.status` | `directory`, `id` | — | returns validated `running`, `stale_state`, PIDs, Java path, start time, and uptime |
| `server.logs` | `directory`, `id` | `cursor`, `file_id`, `max_bytes` (default 65536, max 1048576) | merged console tail; resume with the preceding `next_cursor` + `file_id`; honor `reset` |
| `server.command` | `directory`, `id`, one UTF-8 `command` line | — | writes to the console FIFO; returns `accepted:true` and the validated PID |
| `server.rcon.command` | `directory`, `id`, masked `password`, `command` | `max_bytes` | enabled running server only; result is bounded `text` plus `truncated` |
| `server.query` | `directory`, `id` | `mode` (`basic`/`full`) | enabled running server only; normalized GameSpy4 status |
| `server.properties.get` | `directory`, `id` | — | returns the string-valued `properties` object; `rcon.password` is redacted |
| `server.properties.set` | `directory`, `id`, non-empty `properties` object | — | stopped server only; lifecycle-locked merge and atomic rewrite; returns all resulting keys, with `rcon.password` redacted |
| `server.worlds.list` | `directory`, `id` | `minecraft_version` | read-only; base world from `level-name` plus existing `_nether`/`_the_end` siblings |
| `server.worlds.get` | `directory`, `id`, `world` | `minecraft_version` | read-only; one entry plus `icon_png_base64` |
| `server.worlds.rename` | `directory`, `id`, `world`, `new_name` | — | stopped server only; renaming the `level-name` world also renames existing dimension siblings and rewrites `level-name` in `server.properties` |
| `server.worlds.delete` | `directory`, `id`, `world` | — | stopped server only; deleting the base world cascades to `_nether`/`_the_end` siblings |
| `server.worlds.backup` | `directory`, `id` | `world`, `label` | stopped server only; without `world` packs base + existing siblings into one zip under `servers/<id>/backups/`; returns `{backup}` |
| `server.worlds.backups` | `directory`, `id` | — | `{backups[]}` newest first |
| `server.worlds.restore` | `directory`, `id`, `backup` | `replace` | stopped server only; restores every top-level world directory in the zip; `WORLD_EXISTS` unless `replace:true` |
| `server.worlds.backups.delete` | `directory`, `id`, `backup` | — | `BACKUP_NOT_FOUND` for missing files |
| `server.mods.list` | `directory`, `id` | — | read-only; `{entries[], unmanaged[]}` from `servers/<id>/.jmcl/content.json` |
| `server.mods.install` | `directory`, `id`, `project` | `provider`, `api_key`, `version_id`, `with_dependencies`, `store_directory`, `workers`, `retries` | stopped server only; version+loader come from `server.json` (no `minecraft_version`/`loader` params); vanilla fails `SERVER_LOADER_REQUIRED` |
| `server.mods.set-version` | `directory`, `id`, `project`, `version_id` | `api_key`, `store_directory`, `workers`, `retries` | stopped server only; same result shape as `mods.set-version` |
| `server.mods.enable` / `server.mods.disable` | `directory`, `id`, one of `project`/`file` | — | stopped server only; renames to/from `<file>.disabled` |
| `server.mods.remove` | `directory`, `id`, one of `project`/`file` | — | stopped server only; deletes the server link, store object awaits GC |
| `server.mods.adopt` | `directory`, `id` | `store_directory` | stopped server only; registers unmanaged `mods/*.jar` as `manual` |
| `server.datapacks.list` | `directory`, `id` | — | read-only; `{entries[], unmanaged[]}` from `<level-name>/.jmcl/content.json`; empty until the world exists; requires usable `level-name` properties (`SERVER_PROPERTIES_INVALID` otherwise) |
| `server.datapacks.install` | `directory`, `id`, `project` | `provider`, `api_key`, `version_id`, `with_dependencies`, `store_directory`, `workers`, `retries` | stopped server only; targets `<level-name>/datapacks/`; version from `server.json`; vanilla OK; world dir created on demand; requires usable `level-name` properties normally created by server installation (`SERVER_PROPERTIES_INVALID` otherwise; no separate manifest `installed` check) |
| `server.datapacks.set-version` | `directory`, `id`, `project`, `version_id` | `api_key`, `store_directory`, `workers`, `retries` | stopped server only; same result shape as `datapacks.set-version` |
| `server.datapacks.enable` / `server.datapacks.disable` | `directory`, `id`, one of `project`/`file` | — | stopped server only; renames to/from `<file>.disabled` |
| `server.datapacks.remove` | `directory`, `id`, one of `project`/`file` | — | stopped server only; deletes the world link, store object awaits GC |
| `server.datapacks.adopt` | `directory`, `id` | `store_directory` | stopped server only; registers unmanaged `<level-name>/datapacks/*.zip` as `manual` |

#### State and concurrency matrix

Treat the server as one of three GUI states. `installed` comes from
`server.create|list|get`; `running` comes from `server.status`:

| Operation family | Created, not installed | Installed, stopped | Running |
|---|---:|---:|---:|
| `server.get/list/status` | yes | yes | yes |
| `server.install` | yes | yes (repair/reinstall) | no |
| `server.start` | no | yes | no |
| `server.stop` | succeeds with `termination:not_running` | succeeds with `termination:not_running` | yes |
| `server.restart` | no (completed install required) | yes (starts it) | yes |
| `server.logs` | no | after a prior start created the log | yes |
| `server.command`, RCON, Query | no | no | yes |
| `server.properties.get` | after properties exist | yes | yes |
| `server.properties.set` | after properties exist | yes | no |
| world rename/delete/backup/restore | when required properties/worlds/backup exist; no separate installed-marker gate | yes, with required data | no |
| backup list/delete | yes, when backup exists for deletion | yes | yes |
| server mod mutations | yes (loader manifest required) | yes | no |
| server data-pack mutations | only with usable `level-name` properties | yes | no |
| `server.delete` | yes | yes | no |

The table describes Core permissions, not a requirement to expose every action
in every GUI state. In particular, a stopped `restart` can start a server, and
`stop` is idempotent; the GUI may prefer a Start button while stopped. World
operations validate actual properties/data rather than requiring the installation
marker. Backup listing/deletion do not need a stopped-state guard. The core
re-checks each operation; GUI controls never replace RPC validation. Refresh `server.status` after reconnecting or
after any lifecycle error. Multiple RPC sessions may inspect different
servers concurrently, but never issue concurrent mutations for the same
`directory` + `id`.

#### High-frequency result contracts

`server.status` is the authoritative live-state snapshot:

```json
{"protocol":1,"id":"status-1","event":"result","result":{"stage":"server.status","id":"demo","running":true,"stale_state":false,"pid":12345,"supervisor_pid":12344,"java_path":"/usr/bin/java","started_at_ms":1789740000000,"uptime_ms":3600000}}
```

When `running:false`, `pid`, `supervisor_pid`, `java_path`, `started_at_ms`,
and `uptime_ms` may be `null`. `stale_state:true` means the core removed a dead,
malformed, or identity-mismatched PID state during this request; discard any
GUI assumption that the previous process still exists.

`server.logs` returns a bounded, resumable byte range:

```json
{"protocol":1,"id":"logs-1","event":"result","result":{"stage":"server.logs","id":"demo","text":"[Server thread/INFO]: Done\n","file_id":123456,"start_cursor":0,"next_cursor":33,"eof":true,"reset":false,"truncated":false}}
```

For every next read, send both the preceding `next_cursor` and `file_id`.
`reset:true` means the file rotated, was replaced, or the cursor became
invalid: discard the GUI's partial stream and continue from the returned
`start_cursor`. `truncated:true` means an initial tail omitted older bytes; it
does not mean `text` is invalid UTF-8.

`server.command` only acknowledges FIFO delivery; it does not return command
output:

```json
{"protocol":1,"id":"cmd-1","event":"result","result":{"stage":"server.command","id":"demo","accepted":true,"pid":12345}}
```

Use RCON when the GUI needs a direct textual reply:

```json
{"protocol":1,"id":"rcon-1","method":"server.rcon.command","params":{"directory":"/srv/jmcl/servers","id":"demo","password":"…","command":"list","max_bytes":262144}}
{"protocol":1,"id":"rcon-1","event":"result","result":{"stage":"server.rcon.command","text":"There are 2 of a max of 20 players online","truncated":false}}
```

Query defaults to `mode:"full"`; `version` and `plugins` may be `null`, while
`player_names` is always an array:

```json
{"protocol":1,"id":"query-1","method":"server.query","params":{"directory":"/srv/jmcl/servers","id":"demo","mode":"full"}}
{"protocol":1,"id":"query-1","event":"result","result":{"stage":"server.query","mode":"full","hostname":"A Minecraft Server","game_type":"SMP","map":"world","version":"1.21.4","players":2,"max_players":20,"host_port":25565,"host_ip":"127.0.0.1","plugins":"","player_names":["Alex","Steve"]}}
```

`server.properties.get` and `.set` return string-valued properties. Numeric and
boolean-looking values remain strings. Both methods serialize
`rcon.password` as `"<redacted>"`; a returned redaction marker is never a value
to write back automatically.

#### RCON and Query security

For `server.rcon.command`, the GUI must mask the password input and must never
log, cache, place it in telemetry, or include it in task diagnostics. The core
uses it only for the immediate loopback RCON exchange. RCON is plaintext and
Query may expose player metadata, so the GUI should continue to reach the core
over SSH and should not encourage public exposure of either server port.

The GUI cannot choose a network target. The core always connects to IPv4
`127.0.0.1`; ports are resolved strictly from `server.properties`. RCON
requires `enable-rcon=true`, Query requires `enable-query=true`, and neither
method changes those properties. Do not offer an "expose RCON/Query publicly"
shortcut. RCON and Query are command/status facilities only and do not make a
live-world backup safe.

#### Installation and loader semantics

`server.create`, `server.list`, and `server.get` include `installed`. Treat it
as authoritative. The v3 completion marker must match the manifest and the
recorded server jar plus launch file sizes; absent, malformed, older-schema,
identity-mismatched, or incomplete state returns `false`. At create time use at
most one loader field: `fabric_loader`, `neoforge_version`, or
`forge_version`. Vanilla, Fabric, NeoForge, and modern Forge install on Linux.
Legacy Forge server support remains deferred and returns
`SERVER_LOADER_UNSUPPORTED`. `server.install` requires explicit
`accept_eula:true`; the core never accepts Mojang's EULA implicitly. A
manifest-level `java_path` wins over request `java_paths`. Properties are
strings, including numeric and boolean-looking values.

For server installs, the only progress events are the existing download
events. NeoForge and modern Forge may spend time in a local processor-chain
phase after downloads, but that phase is silent and does not emit
`processor_started` or any server-specific event. Do not wait for a new event
type; wait for the terminal `result` or `error` for the `server.install`
request.

#### Detached lifecycle and log streaming

Running servers are detached from `jmcl-core rpc`; ending or reconnecting SSH
must not be treated as a stop. Keep the `next_cursor` and `file_id` from every
`server.logs` result. If `reset:true`, discard any GUI-side partial stream and
continue from the returned `start_cursor`; `truncated:true` means the initial
tail omitted older bytes. `stale_state:true` means the core removed a dead or
identity-mismatched PID file. Start/restart errors the GUI should surface
distinctly include `SERVER_NOT_INSTALLED`, `SERVER_ALREADY_RUNNING`,
`JAVA_NOT_FOUND`, and `JAVA_INCOMPATIBLE`. A command while stopped returns
`SERVER_NOT_RUNNING`; stop instead succeeds with `termination:"not_running"`
and `pid:null`. Restart accepts a stopped, completed installation and starts it.

#### Server error routing

At minimum, give these stable codes distinct GUI handling:

| Codes | GUI action |
|---|---|
| `INVALID_PARAMS` | local validation error; do not retry unchanged input |
| `UNSUPPORTED_PLATFORM` | disable server management on this host |
| `SERVER_NOT_FOUND`, `SERVER_EXISTS` | refresh the server list |
| `SERVER_NOT_INSTALLED` | offer install/repair |
| `SERVER_ALREADY_RUNNING` | refresh status; disable stopped-only action |
| `SERVER_NOT_RUNNING` | refresh status; disable command/RCON/Query while stopped (not a stop error) |
| `SERVER_STATUS_FAILED` | show host/filesystem failure; status is unknown |
| `EULA_NOT_ACCEPTED` | require an explicit user EULA decision |
| `SERVER_LOADER_UNSUPPORTED`, `SERVER_LOADER_REQUIRED` | change loader/server choice |
| `JAVA_NOT_FOUND`, `JAVA_INCOMPATIBLE` | open Java selection or managed-runtime flow |
| `SERVER_PROPERTIES_NOT_FOUND`, `SERVER_PROPERTIES_INVALID` | offer install/repair or manual property correction |
| `SERVER_LOGS_NOT_FOUND` | show an empty/not-yet-created log state |
| `SERVER_RCON_DISABLED`, `SERVER_QUERY_DISABLED` | explain that the operator must enable and restart the server |
| `SERVER_RCON_AUTH_FAILED` | request the password again; never include it in diagnostics |
| `SERVER_RCON_UNAVAILABLE`, `SERVER_QUERY_UNAVAILABLE` | one manual retry is reasonable after checking live status |
| `SERVER_RCON_PROTOCOL_ERROR`, `SERVER_QUERY_PROTOCOL_ERROR` | report an incompatible/malformed server response; do not loop-retry |
| `WORLD_NOT_FOUND`, `WORLD_EXISTS`, `BACKUP_NOT_FOUND` | refresh the relevant world/backup list |

Messages remain display-only and may change. Validation, disabled-feature,
authentication, and protocol errors are not automatic-retry candidates.

Content methods exist for three kinds with identical shapes:
`mods.*`, `resourcepacks.*`, `shaderpacks.*`. Data packs are the fourth kind
but world-scoped and covered separately below the table.

| Method | Required params | Optional params | Notes |
|---|---|---|---|
| `<kind>.list` | `directory` | — | `{entries[], unmanaged[]}` |
| `<kind>.install` | `directory`, `minecraft_version`, `project` | `version_id`, `provider`, `api_key`, `with_dependencies` (default true), `store_directory`, `workers`, `retries` | `mods.*` also requires `loader` (`fabric`\|`neoforge`\|`forge`); other kinds take no loader |
| `<kind>.set-version` | `directory`, `minecraft_version`, `project`, `version_id` | `loader` (mods only), `api_key`, `store_directory`, `workers`, `retries` | exact version, upgrade or downgrade; provider from registry; no-op when unchanged (`changed:false`) |
| `<kind>.enable` / `<kind>.disable` | `directory` + exactly one of `project`, `file` | — | idempotent; `{changed, entry}` |
| `<kind>.remove` | `directory` + exactly one of `project`, `file` | — | deletes instance link; store object GC'd later |
| `<kind>.adopt` | `directory` | `store_directory` | imports unmanaged files as `manual` entries |

Registry entries (returned by list/install and stored in
`.minecraft/.jmcl/content.json`):

```json
{"kind":"mod","file":"mods/sodium.jar","sha1":"…","size":1306799,
 "source":{"provider":"modrinth","project_id":"AANobbMI","version_id":"c3YkZvne","slug":"sodium"},
 "enabled":true,"as_dependency":false}
```

Semantics the GUI must respect:

- `provider` is `modrinth` | `curseforge` | `manual`. CurseForge ids are
  numeric strings. `manual` entries (adopted files) have no ids and cannot be
  `set-version`ed.
- **Disabled entries live on disk as `<file>.disabled`**; the registry keeps
  the canonical path. Do not "repair" or delete `.disabled` files from the
  GUI — use `<kind>.enable`/`.remove`.
- `as_dependency:true` marks entries pulled in as required dependencies.

Data packs (`datapacks.list|install|set-version|enable|disable|remove|adopt`)
attach to one world instead of the instance: every method takes the game
`directory` plus a required `world` name. Only `install` and `set-version`
also require `minecraft_version` (no `loader`); list, toggle, remove and adopt
do not take a Minecraft version (verified against Core `5a4310de`,
`src/protocol/datapacks.zig`). It manages `saves/<world>/datapacks/*.zip`, and keeps its registry at
`saves/<world>/.jmcl/content.json` — the entry shape is identical but
relative to the world. Results echo `world`. Mutations refuse a live-game
world (`WORLD_LOCKED`, same `session.lock` probe as world mutations) and a
missing world (`WORLD_NOT_FOUND`); `list` never locks. The registry rides
with the world directory (rename/duplicate carry it; export/backup archives
exclude `.jmcl`, so restored datapacks come back unmanaged — offer
`datapacks.adopt` after a restore). Directory-shaped data packs are not
managed content; surface them read-only at most.

`modpack.install` creates a whole instance from an archive:

```json
{"protocol":1,"id":"mp-1","method":"modpack.install","params":{"directory":"/path/to/instances","id":"my-pack","file":"/path/to/pack.mrpack","api_key":null,"store_directory":".jmclcore","workers":8,"retries":2}}
```

Auto-detects Modrinth `.mrpack` vs CurseForge `manifest.json` packs; the
latter requires `api_key`. Result: `{id,name,version_id,loader:{name,version}|null,files_installed,files_skipped,overrides_written}`.
Afterwards run `install.execute` + `install.prepare` for the new instance
(these are not chained).

`store.gc` reclaims unshared cache objects; defaults to dry-run:

```json
{"protocol":1,"id":"gc-1","method":"store.gc","params":{"store_directory":".jmclcore","dry_run":true,"min_age_seconds":86400}}
```

## 5. Caller-Supplied Secrets

The following are **provided by you, the GUI — the core's direct caller**.
The core has none of them built in and never embeds, persists, or echoes
them. Persist reusable OAuth/API credentials in the OS keychain. The RCON
password is the deliberate exception: collect it for the immediate command
and do not cache or persist it. The CurseForge API key is your application's
credential, passed to the core on each call that needs it, exactly like the
Entra client ID.

| Secret | Where used | Format |
|---|---|---|
| Microsoft Entra public client ID | all `auth.microsoft.*` / `auth.minecraft.*` methods as `client_id` | UUID `00001111-aaaa-2222-bbbb-3333cccc4444` |
| CurseForge API key | `provider:"curseforge"` installs, CF modpacks, CF `set-version`, as `api_key` | opaque token from <https://console.curseforge.com/> |
| OAuth refresh tokens | `auth.microsoft.refresh`, `auth.minecraft.refresh_exchange` | owned by GUI keychain, rotated on every refresh |
| RCON password | `server.rcon.command` as `password` | transient opaque string; masked input, never persisted/logged/cached/telemetried |

Error events never contain tokens, keys, device codes, or response bodies.
Apply the same discipline in the GUI: never log raw request/result lines of
auth methods.

## 6. Microsoft Login Flow (device code, RFC 8628)

1. `auth.microsoft.device.begin {client_id, locale?}` → result with
   `device_code` (keep private), `user_code` + verification URI (display),
   expiry, poll interval.
2. Loop `auth.microsoft.device.poll {client_id, device_code}` at the given
   interval. `state` is `authorization_pending` | `slow_down` (+5 s to later
   intervals) | `authenticated` (OAuth credential with access + refresh
   tokens).
3. Immediately persist the refresh token (keychain), then
   `auth.minecraft.exchange {client_id, access_token}` → launch `session`
   (directly usable as `launch.execute`'s `auth`).
4. On later launches: `auth.minecraft.refresh_exchange {client_id,
   refresh_token}` → rotated `refresh_token` + fresh `session`. **Persist the
   rotated token atomically before using the session.** If the result is
   `exchange_failed`, it still carries the rotated token — persist before
   surfacing the error.
5. `account.microsoft.save|list|get|delete` stores public profile metadata
   (id, player name, client_id, xuid) — no tokens — for the account picker.
   Unlike every other method family, their optional `directory` is a single
   CWD-relative directory *name* (default `accounts`); absolute paths and
   path separators are rejected (`INVALID_PARAMS`). See `docs/accounts.md`.
   `save` requires `{account:{id,player_name,client_id,xuid?}}`, not flattened
   profile fields. `xuid` may be null. Its result is `{stage:"save",account}`;
   `get` returns `{stage:"account_get",account}`, not a bare profile. The GUI
   must unwrap the envelope; mocks must preserve these same shapes.

If the IPC channel breaks mid-rotation, treat the account as needing recovery
(new device flow); do not assume the old refresh token is valid.

Stable auth error codes include `AUTH_NETWORK_ERROR`, `AUTH_TIMEOUT`,
`AUTH_RATE_LIMITED`, `AUTHORIZATION_DECLINED`, `XBOX_ACCOUNT_REQUIRED`,
`MINECRAFT_ACCESS_DENIED`, `GAME_NOT_OWNED`, `PROFILE_NOT_FOUND`.

## 7. Error Handling

Every failure is an `error` event with a stable `code` and a human message.
Codes are part of the contract; messages are not (display, don't parse).
Common codes:

- Generic: `INVALID_REQUEST`, `INVALID_PARAMS`, `METHOD_NOT_FOUND`,
  `UNSUPPORTED_PROTOCOL`, `INSTALL_ROOT_ERROR`, `STORE_ROOT_ERROR`,
  `HTTP_STATUS_ERROR`, `RESPONSE_TOO_LARGE`.
- Java: `JAVA_NOT_FOUND`, `JAVA_INCOMPATIBLE`, `JAVA_DOWNLOAD_FAILED`
  (managed runtime download inside `launch.execute` or `install.execute`).
- Content: `NO_COMPATIBLE_VERSION`, `INCOMPATIBLE_VERSION`,
  `VERSION_PROJECT_MISMATCH`, `CONTENT_CONFLICT`, `CONTENT_KIND_MISMATCH`,
  `CONTENT_NOT_FOUND`, `UNSUPPORTED_PROJECT_TYPE`, `DOWNLOAD_UNAVAILABLE`,
  `CONTENT_INSTALL_FAILED`, `CONTENT_SET_VERSION_FAILED`,
  `CONTENT_TOGGLE_FAILED`, `CONTENT_REMOVE_FAILED`, `CONTENT_ADOPT_FAILED`,
  `CONTENT_LIST_FAILED`.
- Modpacks: `INSTANCE_EXISTS`, `UNKNOWN_MODPACK_FORMAT`,
  `UNSUPPORTED_MODPACK_FORMAT`, `UNSUPPORTED_LOADER`, `LOADER_CONFLICT`,
  `MODPACK_NOT_FOUND`, `MODPACK_INSTALL_FAILED`, `UNSAFE_MODPACK`.
- Store: `STORE_GC_PARTIAL` (result contains per-error counts).
- Worlds: `WORLD_NOT_FOUND`, `WORLD_EXISTS`, `WORLD_LOCKED`,
  `WORLD_INVALID_LEVEL_DATA`, `WORLD_ICON_TOO_LARGE`, `WORLD_LIST_FAILED`,
  `WORLD_GET_FAILED`, `WORLD_RENAME_FAILED`, `WORLD_COPY_FAILED`,
  `WORLD_DELETE_FAILED`.
- Auth: see §6.

Retry guidance: network-flavored codes (`HTTP_STATUS_ERROR`, TLS failures)
are worth one retry — the user's typical network has heterogeneous edge
nodes. Validation codes are never retryable.

## 8. What the Core Does NOT Do (GUI responsibilities)

- **Catalog search/browse**: no Modrinth/CurseForge search RPCs by design.
  The GUI calls those HTTP APIs directly (Modrinth needs no key; CurseForge
  uses your key) and passes resolved project/version ids to the core.
- **Credential storage**: OS keychain is the GUI's job.
- **`options.txt` editing**: not a Core RPC. The frontend may use its guarded
  native configuration adapter for supported resource-pack/Iris/Oculus formats;
  unknown formats require explicit in-game/manual activation, not blind writes.
- **Shader runtime selection**: installing Iris/Oculus/OptiFine is just
  mod installation; deciding which one an instance needs is a GUI decision.
- **Concurrent task management** (protocol v1): the GUI multiplexes sessions.
  Closing a dialog/backgrounding a task is not cancellation. In-memory task
  tracking is not restart recovery. There is no task-status/recovery/cancel RPC
  in this contract; do not advertise one or infer success after a broken pipe.

## 9. Shipping the Core Binary

```sh
zig build -Doptimize=ReleaseSafe                          # native
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-windows  # cross
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux
zig build -Doptimize=ReleaseSafe -Dtarget=aarch64-macos
```

Binary: `zig-out/bin/jmcl-core` (single file, no runtime dependencies).
Default paths are CWD-relative (`instances/`, `.minecraft`, `.jmclcore`); the
GUI should pass absolute `directory`/`store_directory` params everywhere
except the account registry (see §6), and pick one shared `store_directory`
so all instances dedupe into one artifact store. Honor `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` — the core reads them.

## 10. CLI Equivalents (debugging)

Every RPC operation has a CLI twin; use it to reproduce GUI issues:

```sh
jmcl-core instance create my-id --name "Test" --version 1.21.4 --fabric-loader 0.16.10
jmcl-core instance install my-id
jmcl-core instance mods install my-id --project sodium
jmcl-core instance mods set-version my-id --project sodium --version FRXt5xaI
jmcl-core instance mods disable my-id --project sodium
jmcl-core instance modpack install my-pack --file pack.mrpack
jmcl-core instance worlds list my-id
jmcl-core instance worlds duplicate my-id "My World" Backup
jmcl-core instance launch my-id
jmcl-core store gc                 # dry-run by default; add --execute to reclaim
```

## 11. Stability Policy

Protocol v1 is unstable until the first tagged release, but every intentional
breaking change increments `protocol`. Additive changes (new methods, new
optional params, new event fields) may land without a bump — ignore unknown
fields everywhere.

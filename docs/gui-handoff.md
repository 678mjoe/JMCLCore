# JMCL Core — GUI Integration Contract

Audience: the agent/developer building the JMCL GUI in a separate repository.
This document is self-contained; reading the core's source is not required.
Details beyond the contract live in `docs/*.md` of the core repository
(`protocol.md`, `content.md`, `modpack.md`, `authentication.md`, `accounts.md`,
`java.md`, `launch.md`).

JMCL Core is a Minecraft: Java Edition launcher core distributed as a single
native binary (`jmcl-core`). The GUI drives it through a JSON Lines RPC
session. The core owns all filesystem, network, download-verification, and
process-supervision work. The GUI owns presentation, catalog search/browse,
credential storage, and all caller-supplied secrets.

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
  the only abort and is safe (atomic writes everywhere), but treat any
  in-flight credential rotation as needing recovery (see §8).

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
`java_policy` (`auto` default, `local`, `managed`) and `store_directory`;
the launch methods also accept `java_override` (explicit executable, skips
the exact-major/architecture gates, mutually exclusive with `java_policy`).
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

Server instances use a separate root and are Linux-only. A remote GUI starts
`ssh user@host jmcl-core rpc` and uses the same JSON Lines protocol. Every
`server.*` call returns `UNSUPPORTED_PLATFORM` when the core is not running
on Linux:

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
| `server.properties.get` | `directory`, `id` | — | returns the full string-valued `properties` object |
| `server.properties.set` | `directory`, `id`, non-empty `properties` object | — | merges string-valued keys and atomically rewrites the file; returns the full resulting object |

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

Running servers are detached from `jmcl-core rpc`; ending or reconnecting SSH
must not be treated as a stop. Keep the `next_cursor` and `file_id` from every
`server.logs` result. If `reset:true`, discard any GUI-side partial stream and
continue from the returned `start_cursor`; `truncated:true` means the initial
tail omitted older bytes. `stale_state:true` means the core removed a dead or
identity-mismatched PID file. Start/restart errors the GUI should surface
distinctly include `SERVER_NOT_INSTALLED`, `SERVER_ALREADY_RUNNING`,
`JAVA_NOT_FOUND`, and `JAVA_INCOMPATIBLE`; stop/command while stopped return
`SERVER_NOT_RUNNING`.

Content methods exist for three kinds with identical shapes:
`mods.*`, `resourcepacks.*`, `shaderpacks.*`.

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
them; you supply them per request and store them (OS keychain recommended).
This applies equally to the CurseForge API key: it is your application's
credential, passed to the core on each call that needs it, exactly like the
Entra client ID.

| Secret | Where used | Format |
|---|---|---|
| Microsoft Entra public client ID | all `auth.microsoft.*` / `auth.minecraft.*` methods as `client_id` | UUID `00001111-aaaa-2222-bbbb-3333cccc4444` |
| CurseForge API key | `provider:"curseforge"` installs, CF modpacks, CF `set-version`, as `api_key` | opaque token from <https://console.curseforge.com/> |
| OAuth refresh tokens | `auth.microsoft.refresh`, `auth.minecraft.refresh_exchange` | owned by GUI keychain, rotated on every refresh |

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
- Auth: see §6.

Retry guidance: network-flavored codes (`HTTP_STATUS_ERROR`, TLS failures)
are worth one retry — the user's typical network has heterogeneous edge
nodes. Validation codes are never retryable.

## 8. What the Core Does NOT Do (GUI responsibilities)

- **Catalog search/browse**: no Modrinth/CurseForge search RPCs by design.
  The GUI calls those HTTP APIs directly (Modrinth needs no key; CurseForge
  uses your key) and passes resolved project/version ids to the core.
- **Credential storage**: OS keychain is the GUI's job.
- **`options.txt` editing**: resource pack / shader activation in-game.
- **Shader runtime selection**: installing Iris/Oculus/OptiFine is just
  mod installation; deciding which one an instance needs is a GUI decision.
- **Concurrent task management/cancellation** (protocol v1): the GUI
  multiplexes sessions.

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
jmcl-core instance launch my-id
jmcl-core store gc                 # dry-run by default; add --execute to reclaim
```

## 11. Stability Policy

Protocol v1 is unstable until the first tagged release, but every intentional
breaking change increments `protocol`. Additive changes (new methods, new
optional params, new event fields) may land without a bump — ignore unknown
fields everywhere.

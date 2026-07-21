# CLI Protocol

## Transport

`jmcl-core rpc` starts a persistent JSON Lines session. Each line is one UTF-8
JSON document. Request lines are limited to 64 KiB in protocol version 1.

- Requests are read from standard input.
- Events are written to standard output and flushed after every event.
- Diagnostics are written to standard error.
- Closing standard input ends the session normally.
- Every message contains `protocol`.
- Request IDs are non-empty strings chosen by the client.

## Request

```json
{"protocol":1,"id":"request-1","method":"ping"}
```

Unknown request fields are ignored to allow compatible protocol extensions.

## Result event

```json
{"protocol":1,"id":"request-1","event":"result","result":{"pong":true}}
```

## Error event

```json
{"protocol":1,"id":"request-1","event":"error","error":{"code":"METHOD_NOT_FOUND","message":"Unknown RPC method"}}
```

Malformed requests that do not provide a readable ID use `null` as the event
ID. One invalid request does not terminate the session.

## Initial methods

| Method | Purpose |
| --- | --- |
| `ping` | Verify that the core is responsive. |
| `core.version` | Query the core name, version, and protocol version. |
| `version.list` | Fetch and filter the Mojang version manifest. |
| `version.get` | Fetch and verify detailed metadata for a version ID. |
| `install.plan` | Generate a validated, platform-specific download plan. |
| `install.execute` | Download and verify every static file in a generated plan. |
| `install.prepare` | Prepare legacy natives and projected assets from the cache. |
| `store.gc` | Report or reclaim unlinked shared artifact cache objects. |
| `java.detect` | Probe locally available Java runtimes. |
| `java.select` | Select a runtime compatible with a Minecraft version. |
| `launch.plan` | Generate Java and game argv without starting a process. |
| `launch.execute` | Prepare, start, and supervise the Minecraft process. |
| `auth.microsoft.device.begin` | Start Microsoft device-code authorization. |
| `auth.microsoft.device.poll` | Perform one Microsoft device-code poll. |
| `auth.microsoft.refresh` | Refresh a Microsoft OAuth credential. |
| `auth.minecraft.refresh_exchange` | Refresh and exchange a credential for a Minecraft launch session. |
| `auth.minecraft.exchange` | Exchange Microsoft access for a Minecraft launch session. |
| `account.microsoft.save` | Atomically save public Microsoft account metadata. |
| `account.microsoft.list` | List public Microsoft account metadata. |
| `account.microsoft.get` | Read one public Microsoft account record. |
| `account.microsoft.delete` | Remove one public Microsoft account record. |

## Fabric Selection

`install.plan`, `install.execute`, `install.prepare`, `launch.plan`, and
`launch.execute` accept an optional `fabric_loader` string alongside the
Minecraft `id`. It pins the Fabric Loader version for that request:

```json
{"protocol":1,"id":"fabric-plan","method":"install.plan","params":{"id":"1.21.1","fabric_loader":"0.16.10"}}
```

The core resolves and verifies the official Fabric profile before emitting a
plan. Fabric API and third-party mod downloads are outside this protocol scope.

## NeoForge Selection

The same five methods accept an optional `neoforge_version` string instead of
`fabric_loader`; at most one loader field may be set per request.
`install.execute` with `neoforge_version` also accepts optional `java_paths`
and runs the NeoForge processor chain after the download stage. Its result then
includes `processors_run` and the `patched_client` SHA-1:

```json
{"protocol":1,"id":"neoforge-install","method":"install.execute","params":{"id":"1.20.4","neoforge_version":"20.4.251"}}
```

See `docs/neoforge.md` for the two-stage installation and processor security
boundaries.

## Forge Selection

The same five methods accept an optional `forge_version` string alongside the
Minecraft `id`, mutually exclusive with `fabric_loader` and `neoforge_version`.
`install.execute` with `forge_version` runs the Forge processor chain after the
download stage, verifies the patched client against the install profile's
declared checksum, and reports `processors_run` and the `patched_client`
SHA-1:

```json
{"protocol":1,"id":"forge-install","method":"install.execute","params":{"id":"1.20.4","forge_version":"49.2.8"}}
```

NeoForge and Forge emit one `processor_started` event before each applicable
installer processor, with the same progress fields printed by the CLI:

```json
{"protocol":1,"id":"forge-install","event":"processor_started","source":"official","processor_started":{"stage":"processor","index":2,"total":4,"name":"net.minecraftforge:installertools:1.4.1"}}
```

See `docs/forge.md` for details.

## Mod Management

`mods.list`, `mods.install`, `mods.remove`, and `mods.adopt` manage Modrinth
mods inside a `.minecraft` directory. `mods.install` resolves the newest
compatible release (or an exact `version_id`) plus its required dependencies
against the given `minecraft_version` and `loader`, downloads through the
verified content-addressed pipeline, and reports the installed entries:

```json
{"protocol":1,"id":"mods-install","method":"mods.install","params":{"directory":"/path/to/.minecraft","minecraft_version":"1.20.4","loader":"fabric","project":"sodium"}}
```

`loader` is one of `fabric`, `neoforge`, or `forge`. `mods.set-version`
switches a registered entry to an exact caller-selected version id (upgrade
or downgrade) and takes no `provider` — the registry entry determines it;
CurseForge entries pass `api_key` and a numeric file id. `mods.enable` and
`mods.disable` toggle a registered entry (same selectors as `mods.remove`);
disabled entries are renamed to `<file>.disabled` on disk so the game does
not load them. `mods.remove` takes
exactly one of `project` or `file`. `mods.adopt` imports unmanaged `mods/`
jars into the artifact store and registers them with the `manual` provider.

`resourcepacks.list`, `resourcepacks.install`, `resourcepacks.remove`, and
`resourcepacks.adopt` mirror the mod methods for resource packs. They take no
`loader` — compatibility resolves through Modrinth's `minecraft`
pseudo-loader — and manage `resourcepacks/*.zip` files. Vanilla directories
are supported:

```json
{"protocol":1,"id":"rp-install","method":"resourcepacks.install","params":{"directory":"/path/to/.minecraft","minecraft_version":"1.21.1","project":"default-dark-mode"}}
```

`shaderpacks.list`, `shaderpacks.install`, `shaderpacks.remove`, and
`shaderpacks.adopt` mirror them again for shader packs, managing
`shaderpacks/*.zip` files. Shader versions carry shader-loader tags
(`iris`, `optifine`, …) rather than the `minecraft` pseudo-loader, so
`shaderpacks.install` takes no `loader` at all and filters by
`minecraft_version` only; picking a shader runtime is the embedding
application's job.

```json
{"protocol":1,"id":"sp-install","method":"shaderpacks.install","params":{"directory":"/path/to/.minecraft","minecraft_version":"1.21.4","project":"bsl-shaders"}}
```

All install methods accept an optional `provider` (`"modrinth"` by default,
or `"curseforge"`). The CurseForge provider resolves projects by numeric mod
id and requires an `api_key` parameter, supplied by the caller per request
and never persisted or echoed by the core:

```json
{"protocol":1,"id":"cf-install","method":"mods.install","params":{"directory":"/path/to/.minecraft","minecraft_version":"1.20.4","loader":"fabric","provider":"curseforge","api_key":"$2a$10$…","project":"238222"}}
```

All install methods default to recursing required dependencies; pass
`"with_dependencies":false` to install only the requested project. See
`docs/content.md`.

## Instance Management

`instance.create`, `instance.list`, `instance.get`, and `instance.delete`
manage instances under a shared instances root (`directory`). `create`
resolves the requested Minecraft version and at most one loader field
(`fabric_loader`, `neoforge_version`, `forge_version`) online before writing
the manifest, and fails with `INSTANCE_EXISTS` when the id is taken:

```json
{"protocol":1,"id":"ic-1","method":"instance.create","params":{"directory":"/path/to/instances","id":"my-instance","name":"My Instance","version_id":"1.21.4","fabric_loader":"0.16.10"}}
```

Results carry the manifest fields (`id`, `name`, `version_id`, loader
fields, `source`) plus `installed`. `instance.create` always returns
`installed:false`; `instance.list` and `instance.get` read the current
installation status. `instance.get` and `instance.delete` address one
instance by `id` and report `INSTANCE_NOT_FOUND` when absent. Deletion removes
the whole instance directory. Game installation remains a separate
`install.execute` step.

`installed:true` means a complete `install.execute` (including the
NeoForge/Forge processor chain when applicable) last succeeded for the exact
version, loader, and source in the instance manifest. Completion is recorded
atomically in `.minecraft/.jmcl/install.json`; an absent, malformed, or
mismatched marker returns `false`. Instances installed before this marker was
introduced conservatively return `false` and can be reinstalled
idempotently—the verified artifact store and hard-link materialization avoid
duplicate downloads and storage.

## Modpack Installation

`modpack.install` creates a new instance from a Modrinth `.mrpack` or
CurseForge `manifest.json` archive: it pins the Minecraft version and mod
loader, downloads every client-side content file through the verified
content-addressed pipeline, registers them in the content registry, and
extracts the pack's overrides. CurseForge packs require an `api_key`.

```json
{"protocol":1,"id":"mp-1","method":"modpack.install","params":{"directory":"/path/to/instances","id":"my-pack","file":"/path/to/pack.mrpack"}}
```

The result reports the instance identity, the selected loader,
`files_installed`, `files_skipped`, and `overrides_written`. See
`docs/modpack.md`.

## `store.gc`

`store.gc` performs conservative cache reclamation. It defaults to dry-run mode
and a 24-hour minimum age. Dry-run is read-only; an execute request deletes an
object only after its digest lock is held and its hard-link count remains one.

```json
{"protocol":1,"id":"gc-1","method":"store.gc","params":{"store_directory":".jmclcore","dry_run":true,"min_age_seconds":86400}}
```

The result contains `dry_run`, `min_age_seconds`, and a summary with scanned,
eligible, deleted, linked, young, ignored, busy, `errors`, and byte counters.
When `errors` is nonzero, `store.gc` returns `STORE_GC_PARTIAL` instead of a
result event.

## `version.list`

All parameters are optional. `type` accepts `release`, `snapshot`, `old_beta`,
or `old_alpha`. `limit` defaults to 20 and must be between 1 and 1000. `source`
accepts `official` or `bmclapi` and defaults to `official`.

```json
{"protocol":1,"id":"request-2","method":"version.list","params":{"type":"release","limit":2,"source":"bmclapi"}}
```

The result contains Mojang's latest release and snapshot IDs followed by the
selected version entries:

```json
{"protocol":1,"id":"request-2","event":"result","result":{"latest":{"release":"1.21.6","snapshot":"25w20a"},"versions":[{"id":"1.21.6","type":"release","url":"https://example.invalid/1.21.6.json","time":"2025-06-17T12:00:00+00:00","releaseTime":"2025-06-17T12:00:00+00:00","sha1":"0123456789abcdef","complianceLevel":1}]}}
```

Fetching honors the standard `HTTP_PROXY`, `HTTPS_PROXY`, and `ALL_PROXY`
environment variables, including their lowercase forms.

## Authentication RPC

Authentication result events contain credentials and must be treated as
secret-bearing IPC. Callers must not log, persist in ordinary files, or expose
raw request and result lines. Error events never contain tokens, device codes,
or service response bodies.

`auth.microsoft.device.begin` requires a configured public-client ID and
optionally accepts a locale:

```json
{"protocol":1,"id":"auth-1","method":"auth.microsoft.device.begin","params":{"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","locale":"zh-CN"}}
```

Its result contains `device_code`, `user_code`, URI data, expiry, polling
interval, and optional message. Display only the user code and URI; retain the
device code privately until the flow completes or expires.

`auth.microsoft.device.poll` performs exactly one poll. The caller schedules
subsequent calls according to the returned device interval and RFC 8628:

```json
{"protocol":1,"id":"auth-2","method":"auth.microsoft.device.poll","params":{"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","device_code":"secret"}}
```

Pending results have `state` `authorization_pending` or `slow_down`. A
successful poll has `state` `authenticated` and an OAuth credential containing
the access and refresh tokens. `slow_down` requires the caller to add five
seconds to all later poll intervals for that flow.

`auth.microsoft.refresh` accepts `client_id` and `refresh_token`; its result
contains a refreshed credential. Persist the returned refresh token atomically
before discarding the previous one.

```json
{"protocol":1,"id":"auth-3","method":"auth.microsoft.refresh","params":{"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","refresh_token":"secret"}}
```

`auth.minecraft.refresh_exchange` is a convenience normal-launch operation. It
accepts the same `client_id` and `refresh_token`, refreshes Microsoft OAuth,
performs the Xbox/XSTS/Minecraft exchange, and does not return the intermediate
Microsoft access token. A successful result has `state` `authenticated`, a
rotated `refresh_token`, and a launch-compatible Minecraft `session`.

```json
{"protocol":1,"id":"auth-4","method":"auth.minecraft.refresh_exchange","params":{"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","refresh_token":"secret"}}
```

Persist the returned refresh token atomically before launching the returned
session. If refresh succeeds but a downstream exchange step fails, the result
instead has `state` `exchange_failed`, the rotated `refresh_token`, and a fixed
`exchange_error.code`; it has no session. Persist the token before surfacing or
retrying that exchange error. If the session is no longer needed, discard it
rather than persisting its short-lived Minecraft access token.

Credential rotation cannot be made atomic across a broken stdin/stdout IPC
channel without a core-owned secret store. If the caller cannot read a complete
terminal result due to process or pipe failure, it must treat the account as
requiring recovery or a new device authorization rather than assuming the old
refresh token remains valid.

`auth.minecraft.exchange` accepts a Microsoft access-only credential and
returns a Minecraft launch session with profile, Minecraft access token,
expiry, optional XUID, and client ID:

```json
{"protocol":1,"id":"auth-5","method":"auth.minecraft.exchange","params":{"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","access_token":"secret"}}
```

The `session` result is directly compatible with the `auth` field of
`launch.plan` or `launch.execute`; ignore its additional `expires_in` field.
The core validates Xbox access, Minecraft Java ownership, and the Java profile
during this exchange.

Authentication errors use fixed error codes such as `AUTH_NETWORK_ERROR`,
`AUTH_TIMEOUT`, `AUTH_RATE_LIMITED`, `AUTHORIZATION_DECLINED`,
`XBOX_ACCOUNT_REQUIRED`, `MINECRAFT_ACCESS_DENIED`, `GAME_NOT_OWNED`, and
`PROFILE_NOT_FOUND`. See `docs/authentication.md` for the complete policy.

## Account Registry

The account registry stores public profile metadata only and never includes
refresh tokens or access tokens. `account.microsoft.save`, `.list`, `.get`, and
`.delete` manage those records. The caller owns durable credentials through a
platform secret store. See `docs/accounts.md` for the record format and refresh
workflow.

## `version.get`

The required `id` must exactly match an entry in the selected version manifest.
The optional `source` accepts `official` or `bmclapi` and defaults to `official`.

```json
{"protocol":1,"id":"request-3","method":"version.get","params":{"id":"1.21.11","source":"bmclapi"}}
```

The core fetches the URL from the manifest, verifies the response against the
manifest SHA-1, parses the metadata, and verifies that the returned ID matches
the requested ID. The result includes:

- Client and optional server or mapping downloads.
- Asset index metadata.
- Libraries, classifiers, native mappings, extraction exclusions, and rules.
- Modern game, JVM, and default-user-JVM arguments.
- Legacy `minecraftArguments` when present.
- Java runtime and logging requirements when present.

The normalized result may include `null` for fields omitted by a particular
Minecraft generation. Argument entries preserve Mojang's untagged string and
conditional-object JSON representation.

## Download sources

Successful `version.list` and `version.get` events include a top-level `source`
field so a GUI can visibly identify the active provider:

```json
{"protocol":1,"id":"request-3","event":"result","source":"bmclapi","result":{}}
```

The BMCLAPI source rewrites Mojang metadata and object URLs to
`https://bmclapi2.bangbang93.com`. It also defines mappings for asset objects
and Maven libraries for the installation stage. Content carrying an official
SHA-1 is always verified against that value.

Applications using BMCLAPI must visibly identify BMCLAPI as the source, as
required by its service policy. See <https://bmclapidoc.bangbang93.com/>.

## `install.plan`

`id` is required. `source`, `os`, and `arch` are optional. The target defaults
to the platform running JMCL Core.

- `source`: `official` or `bmclapi`.
- `os`: `windows`, `linux`, or `osx`.
- `arch`: `x86`, `x86_64`, `arm`, or `arm64`.

```json
{"protocol":1,"id":"request-4","method":"install.plan","params":{"id":"1.6.4","source":"bmclapi","os":"windows","arch":"x86_64"}}
```

The result contains a deduplicated file list including asset objects, an ordered
classpath, legacy native extraction instructions, legacy asset projections, and
total download size. See `docs/install-plan.md`.

## `install.execute`

`id` is required. `directory` defaults to `.minecraft` and `store_directory`
defaults to `.jmclcore`. Platform and source parameters match `install.plan`.
`workers` defaults to 8 and accepts 1 through 32. `retries` defaults to 2 and
accepts 0 through 5.

When `source` is `bmclapi`, metadata planning retries through Mojang if the
mirror is unavailable. Static artifact URLs remain BMCLAPI-first with Mojang
fallback and SHA-1 verification.

```json
{"protocol":1,"id":"install-1","method":"install.execute","params":{"id":"1.21.11","directory":"./.minecraft","store_directory":"./.jmclcore","source":"bmclapi","workers":8,"retries":2}}
```

After planning succeeds, the request emits a `started` event:

```json
{"protocol":1,"id":"install-1","event":"started","source":"bmclapi","started":{"stage":"download","version_id":"1.21.11","files_total":5000,"bytes_total":600000000}}
```

It then emits `file`, `retry`, and aggregate `progress` events. Exactly one
terminal `result` or `error` event follows.

```json
{"protocol":1,"id":"install-1","event":"file","source":"bmclapi","file":{"path":"libraries/example.jar","status":"downloaded","placement":"hard_link","size":1234,"attempts":1}}
{"protocol":1,"id":"install-1","event":"retry","source":"bmclapi","retry":{"path":"assets/objects/ab/abcdef","attempt":1,"max_attempts":3,"error":"UnsuccessfulHttpStatus"}}
{"protocol":1,"id":"install-1","event":"progress","source":"bmclapi","progress":{"files_completed":100,"files_total":5000,"bytes_verified":12000000,"bytes_total":600000000,"bytes_transferred":8000000}}
{"protocol":1,"id":"install-1","event":"result","source":"bmclapi","result":{"stage":"download","version_id":"1.21.11","files_total":5000,"files_downloaded":4500,"files_cached":500,"files_hard_linked":4990,"files_copied":10,"bytes_verified":600000000,"bytes_transferred":550000000}}
```

Only after every planned file and loader processor succeeds,
`install.execute` atomically records `.jmcl/install.json`. Instance
`installed` status therefore never becomes true for a partial install.

RPC execution is synchronous in protocol version 1: the session cannot process
another request until the terminal event. Cancellation will be added with the
asynchronous task layer.

## `install.prepare`

`id` is required and `directory` defaults to `.minecraft`. Source and platform
parameters match `install.plan`. All planned native archives and projected
asset objects must already be present and valid in the cache.

```json
{"protocol":1,"id":"prepare-1","method":"install.prepare","params":{"id":"1.6.4","directory":"./.minecraft","source":"official","os":"windows","arch":"x86_64"}}
```

The result returns a fresh native directory relative to `.minecraft`. Modern versions
receive an empty directory when launcher-managed extraction is unnecessary.

```json
{"protocol":1,"id":"prepare-1","event":"result","source":"official","result":{"stage":"prepare","version_id":"1.6.4","native_directory":".jmcl/runtime/natives/1234abcd","native_files":8,"projections_created":560,"projections_cached":560}}
```

## `java.detect`

The optional `paths` array adds explicit Java executables before platform,
`JAVA_HOME`, and `PATH` discovery.

```json
{"protocol":1,"id":"java-1","method":"java.detect","params":{"paths":["/custom/jdk/bin/java"]}}
```

The result contains successfully probed runtimes and per-candidate failures.
Each probe has bounded output and a ten-second deadline.

## `java.select`

`id` is required. `source`, `arch`, and `paths` are optional. Architecture
defaults to the current host architecture.

```json
{"protocol":1,"id":"java-2","method":"java.select","params":{"id":"26.2","source":"official","arch":"arm64"}}
```

Selection requires the exact Java major from version metadata and a matching
JVM architecture. Metadata without `javaVersion` requires Java 8.

```json
{"protocol":1,"id":"java-2","event":"result","source":"official","result":{"version_id":"26.2","required_major":25,"required_arch":"arm64","runtime":{"executable":"/path/to/java","major_version":25,"os_arch":"aarch64"},"probe_failures":[]}}
```

See `docs/java.md` for discovery and compatibility policy.

## `launch.plan`

This method requires a version ID, native directory returned by
`install.prepare`, and authentication session. `directory` defaults to
`.minecraft` and is used as both the static data root and game working
directory. It automatically selects a compatible local Java runtime.

```json
{"protocol":1,"id":"launch-1","method":"launch.plan","params":{"id":"26.2","directory":"./.minecraft","natives_directory":".jmcl/runtime/natives/session-id","auth":{"player_name":"Player","uuid":"00000000000000000000000000000000","access_token":"secret"},"options":{"resolution_width":1280,"resolution_height":720}}}
```

The public result contains Java and working-directory information plus
`redacted_argv` and `sensitive_argv_indices`. Raw token-bearing arguments remain
internal and are not serialized by this diagnostic method.

See `docs/launch.md` for argument ordering and supported placeholders.

## `launch.execute`

This method accepts a version, one `.minecraft` directory, authentication,
options, source, architecture, and Java candidates. It does not accept a native
directory or client-supplied argv. The core creates and owns the runtime native
directory internally. `directory` defaults to `.minecraft`.

```json
{"protocol":1,"id":"game-1","method":"launch.execute","params":{"id":"26.2","directory":"./.minecraft","auth":{"player_name":"Player","uuid":"00000000000000000000000000000000","access_token":"secret"}}}
```

After spawning Java, the core emits `started` with a numeric PID and redacted
argv. Child output uses bounded Base64 records so arbitrary bytes remain valid
JSON.

```json
{"protocol":1,"id":"game-1","event":"started","source":"official","started":{"stage":"launch","version_id":"26.2","pid":1234,"redacted_argv":["java","...","<redacted>"]}}
{"protocol":1,"id":"game-1","event":"stdout","source":"official","stdout":{"sequence":1,"encoding":"base64","end":"newline","data":"SGVsbG8="}}
{"protocol":1,"id":"game-1","event":"stderr","source":"official","stderr":{"sequence":2,"encoding":"base64","end":"more","data":"Li4u"}}
```

Normal, nonzero, signaled, stopped, and unknown process terminations all produce
a terminal result. `error` is reserved for planning, preparation, spawning, or
supervision failures.

```json
{"protocol":1,"id":"game-1","event":"result","source":"official","result":{"stage":"launch","version_id":"26.2","process":{"pid":1234,"termination":"exited","exit_code":0,"signal":null,"status":null,"duration_ms":120000,"max_rss":536870912},"native_cleanup":"removed"}}
```

Protocol version 1 remains synchronous and has no in-band launch cancellation.
If protocol output fails, the core continues draining and supervising the child,
then cleans its native directory before returning.


The protocol remains unstable until the first tagged release, but every
intentional breaking change must increment the protocol version.

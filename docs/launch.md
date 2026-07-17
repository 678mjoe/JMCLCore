# Launch Plan

`launch.plan` validates and summarizes launch arguments without starting
Minecraft. `launch.execute` builds the private token-bearing argv, starts Java,
and supervises the process.

## Inputs

The caller provides:

- A single `.minecraft` directory for static runtime and mutable game files.
- Native directory returned by `install.prepare`.
- Microsoft/Minecraft profile name, UUID, and access token.
- Optional resolution, demo, quick-play, and extra argument settings.
- Optional explicit Java candidates.

The core resolves metadata, classpath, asset layout, and a compatible Java
runtime itself. Authenticated secrets are not accepted as ordinary CLI flags to
avoid exposing them through the operating-system process list. The CLI supports
a non-secret `--offline-name` for local testing.

## Directory Layout

Installation, preparation, and execution default to `./.minecraft`. A custom
location can be selected with `--directory PATH` or the RPC `directory` field.
The selected directory is both the launcher data root and Minecraft working
directory, so all relevant content remains under one tree:

- `assets/`, `libraries/`, and `versions/` contain verified static files.
- `.jmcl/runtime/natives/` contains launch-owned temporary native directories.
- `logs/`, `saves/`, `resourcepacks/`, and `options.txt` are written by the game.

Managed instances apply this same layout independently at
`instances/ID/.minecraft`; see `docs/instances.md`.

## Argument Order

Modern metadata produces:

1. Metadata `default-user-jvm` arguments when enabled.
2. Metadata JVM arguments filtered by rules.
3. Caller-provided extra JVM arguments.
4. Metadata logging configuration argument.
5. Main class.
6. Metadata game arguments filtered by rules.
7. Caller-provided extra game arguments.

Legacy metadata receives generated `java.library.path` and classpath arguments.
Its `minecraftArguments` string is tokenized with quote and escape handling
before placeholder substitution, so substituted paths containing spaces remain
one argv element.

## Rules

Rules use Mojang's ordered last-match-wins behavior. Supported predicates
include OS name, architecture, OS version patterns, version ranges, demo,
custom resolution, and quick-play features.

The metadata regex subset supports anchors, literals, escaped literals, `\d`,
`.`, and `*`. Unsupported expressions fail planning rather than being silently
treated as matching.

## Placeholders

Supported values include:

- Authentication: player name, UUID, access token, session, XUID, client ID,
  user type, and user properties.
- Layout: game directory, assets root/index, legacy game assets, libraries,
  natives, and classpath.
- Version and launcher name/version.
- Resolution and quick-play values.

An unknown placeholder fails planning so future metadata changes cannot produce
a silently broken command line.

## Secrets

The internal complete `argv` contains the access token required by Minecraft.
The public plan returns:

- `sensitive_argv_indices`: positions containing token/session substitutions.
- `redacted_argv`: the same command with those values replaced by
  `<redacted>`.

Only `redacted_argv` may be written to logs. Raw JVM/game arrays are not exposed
by `launch.plan` and must not be persisted as ordinary instance configuration.

## Process Supervision

`launch.execute` creates the `.minecraft` directory, runs preparation, selects Java,
and spawns the argv directly without a shell. It concurrently drains stdout and
stderr, assigns a merged sequence to emitted records, and reports every process
termination form.

Output is treated as arbitrary bytes and encoded as Base64 in RPC events. Access
token and session values are replaced before line framing, including when a
secret crosses underlying read boundaries.

The child receives an allowlisted environment. Java option injection, dynamic
loader injection, proxy credentials, cloud credentials, and unrelated GUI
environment variables are not inherited.

After the child is reaped, its launch-owned native directory is removed. A
cleanup failure is reported in the terminal result without discarding the
process exit status.

Protocol version 1 launch execution is synchronous and cannot be cancelled from
the same RPC session. An asynchronous launch registry and process-tree
cancellation remain future work.

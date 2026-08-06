# Worlds

JMCL Core manages save/world directories for client instances and exposes
read-only world inspection for managed servers. Worlds are **mutable,
instance-private state**: they never enter the content-addressed artifact
store, are never hard-linked, and are never shared between instances. All
operations are plain filesystem moves and copies with crash-safe staging.

## Storage model

Client worlds live at `instances/<id>/.minecraft/saves/<world>/`. The world
name is the directory name. Server worlds live at `servers/<id>/<level-name>/`
plus the `<level-name>_nether` and `<level-name>_the_end` dimension siblings,
where `level-name` comes from `server.properties` (default `world`).

World names are validated before any operation: 1-255 bytes, no path
separators, no NUL or control characters, not `.`/`..`, no leading dot, and
no trailing space or dot (Windows rules). Names violating this are rejected
with `INVALID_PARAMS`.

## Metadata

`level.dat` is gzip-compressed NBT. The core parses it read-only for listing
and extracts from the `Data` compound:

- `level_name` (`Data.LevelName`)
- `version_name` / `version_id` (`Data.Version.Name` / `Data.Version.Id`;
  absent on pre-1.9 worlds)
- `game_mode` (`Data.GameType`: survival/creative/adventure/spectator)
- `hardcore`, `cheats` (`Data.allowCommands`), `difficulty`,
  `last_played_ms`

A world whose `level.dat` is missing or malformed still lists; all metadata
fields are `null`. `size_bytes` is a recursive regular-file sum, `has_icon`
reports `icon.png` presence, and `locked` reports `session.lock` presence
(the game creates it while a world is open and deletes it on clean exit;
crashes can leave stale locks).

`version_relation` compares the world's `version_name` against a
caller-supplied `minecraft_version`: `same`, `different`, or `unknown`
(either side missing). It is advisory only: `different` covers both upgrades
and downgrades, and snapshot names may coincide with release ids. The core
never blocks on it; downgrade risk messaging is a GUI concern.

## Mutations

All mutations refuse to run while `session.lock` exists (`WORLD_LOCKED`) and
while the target name is taken (`WORLD_EXISTS`).

- **rename**: rewrites `Data.LevelName` inside `level.dat` (full NBT
  parse/serialize with a parse-then-compare round-trip check, atomic file
  replace), then renames the directory. A world without a parseable
  `level.dat` is renamed directory-only.
- **duplicate**: recursive plain-file copy into a `.jmcl-copy-<random>`
  sibling staging directory, excluding `session.lock` and staging/trash
  directories, then atomic rename to the target. Hard links are never used;
  symlinks and other non-regular entries fail the operation
  (`WORLD_COPY_FAILED`).
- **delete**: rename to a `.jmcl-trash-<random>` sibling, then recursive
  delete; on delete failure the directory is moved back. No confirmation is
  requested by the core; confirm destructive actions in the GUI.

Stale `.jmcl-copy-*`/`.jmcl-trash-*` directories left by crashes are removed
on the next `worlds.list`.

## RPC

Client operations take the instance game directory (the same `directory`
shape as `mods.*`):

```json
{"protocol":1,"id":"w-1","method":"worlds.list","params":{"directory":"/path/to/instances/my-instance/.minecraft","minecraft_version":"1.21.4"}}
{"protocol":1,"id":"w-2","method":"worlds.get","params":{"directory":"/path/to/instances/my-instance/.minecraft","world":"My World","minecraft_version":"1.21.4"}}
{"protocol":1,"id":"w-3","method":"worlds.rename","params":{"directory":"/path/to/instances/my-instance/.minecraft","world":"My World","new_name":"New Name"}}
{"protocol":1,"id":"w-4","method":"worlds.duplicate","params":{"directory":"/path/to/instances/my-instance/.minecraft","world":"New Name","new_name":"Backup"}}
{"protocol":1,"id":"w-5","method":"worlds.delete","params":{"directory":"/path/to/instances/my-instance/.minecraft","world":"Backup"}}
```

`minecraft_version` is optional and only feeds `version_relation`; the CLI
fills it from the instance manifest automatically. `worlds.list` returns
`{"worlds":[...]}` sorted by name. `worlds.get` returns one `world` entry
plus `icon_png_base64` (`null` when absent; icons are capped at 64 KiB).
`worlds.rename` and `worlds.duplicate` return the resulting `world` entry;
`worlds.delete` returns `{"deleted":"<name>"}`.

Server parity is read-only and Linux-gated like every `server.*` method:

```json
{"protocol":1,"id":"w-6","method":"server.worlds.list","params":{"directory":"/path/to/servers","id":"my-server"}}
{"protocol":1,"id":"w-7","method":"server.worlds.get","params":{"directory":"/path/to/servers","id":"my-server","world":"world"}}
```

`server.worlds.list` resolves `level-name` from `server.properties` and lists
the base world plus existing `_nether`/`_the_end` siblings; dimension
directories have no `level.dat`, so their metadata fields are `null`.

Error codes: `WORLD_NOT_FOUND`, `WORLD_EXISTS`, `WORLD_LOCKED`,
`WORLD_INVALID_LEVEL_DATA`, `WORLD_ICON_TOO_LARGE`, `WORLD_LIST_FAILED`,
`WORLD_GET_FAILED`, `WORLD_RENAME_FAILED`, `WORLD_COPY_FAILED`,
`WORLD_DELETE_FAILED`; invalid names and out-of-range input use
`INVALID_PARAMS`.

## CLI

```sh
jmcl-core instance worlds list ID [--instances-directory PATH] [--json]
jmcl-core instance worlds show ID WORLD [--instances-directory PATH] [--json]
jmcl-core instance worlds rename ID WORLD NEW_NAME [--instances-directory PATH]
jmcl-core instance worlds duplicate ID WORLD NEW_NAME [--instances-directory PATH]
jmcl-core instance worlds delete ID WORLD [--instances-directory PATH]

jmcl-core server worlds list ID [--servers-directory PATH] [--json]
jmcl-core server worlds show ID WORLD [--servers-directory PATH] [--json]
```

`instance.delete` removes the whole instance tree, so instance worlds need no
registry entries, no GC rules, and no store interaction.

## Out of scope

Zip export/import (`worlds.export`/`worlds.import`) and per-world
backup/restore are planned as a follow-up phase; the zip writer does not
exist yet. Server-side world mutations (rename/delete/backup) ship with that
phase together with the stopped-server requirement.

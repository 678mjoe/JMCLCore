# Worlds

JMCL Core manages save/world directories for client instances and managed
servers. Worlds are **mutable, instance-private state**: they never enter the
content-addressed artifact store, are never hard-linked, and are never shared
between instances. All operations are plain filesystem moves and copies with
crash-safe staging.

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
reports `icon.png` presence, and `locked` probes `session.lock` with a
non-blocking exclusive file lock: it is `true` only while a live process
(the running game) holds the lock. Stale `session.lock` files left behind
after a clean exit or a crash do not mark the world as locked and do not
block mutations; the file must be opened read-write because BSD/macOS
refuse exclusive locks on read-only descriptors
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

## Export, import, and backups

`worlds.export` streams a world into a standard zip archive (deflate,
data-descriptor free, written atomically via temp-name + rename). Entries are
prefixed with the world directory name; `session.lock`, staging/trash
directories, and the `.jmcl` metadata directory (the world-scoped datapack
content registry, see `docs/content.md`) are excluded. The zip writer is
hand-rolled
(`src/worlds/zip_writer.zig`) with the same limits as the reader: at most
65535 entries, 4096-byte entry names, 4 GiB per file and per archive.
Exporting a locked world fails with `WORLD_LOCKED`.

`worlds.import` extracts a world zip into `saves/`. Safety rules: entries
with absolute paths, `..` components, backslashes, drive letters, or NUL
bytes are rejected (`WORLD_ARCHIVE_INVALID`); archives over 1 GiB compressed
fail with `WORLD_ARCHIVE_TOO_LARGE`. When every entry shares one top-level
directory the prefix is stripped and becomes the default world name; a
caller `name` overrides it, and without a prefix the zip file stem is used.
Extraction stages into `.jmcl-copy-<random>` and renames into place; an
existing target fails with `WORLD_EXISTS` unless `replace:true`, which swaps
the old directory to `.jmcl-trash-<random>` first (full replacement: files
added since the backup do not survive).

`worlds.backup` is managed export into the `backups/` directory next to
`saves/` (i.e. `<game dir>/backups/`), named
`<world>-<YYYYmmdd-HHMMSS>[-label].zip`. The label is sanitized to
`[A-Za-z0-9._-]` (max 40 bytes). `worlds.backups` lists that directory
(`{file, world, created_ms, size_bytes}`, newest first; `world` is parsed
from the file name and `null` for foreign zips). `worlds.restore` is
`worlds.import` from the backups directory and `worlds.backups.delete`
removes one backup (`BACKUP_NOT_FOUND`).

Server worlds support the same archive operations plus mutations, all gated
on the stopped-server lifecycle guard (`SERVER_ALREADY_RUNNING`):

- **server.worlds.rename / server.worlds.delete**: renaming the base world
  (the `level-name` directory) also renames existing `_nether`/`_the_end`
  siblings and rewrites `level-name` in `server.properties`; deleting the
  base world cascades to those siblings. Renaming or deleting a sibling
  alone touches only that directory.
- **server.worlds.backup** without a `world` packs the base world plus all
  existing dimension siblings into one zip under `servers/<id>/backups/`;
  with `world` it packs that directory only.
- **server.worlds.restore** requires every entry to sit under a top-level
  world directory and restores each one (staging + rename, `replace`
  semantics as above).

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

Archive operations:

```json
{"protocol":1,"id":"w-8","method":"worlds.export","params":{"directory":"/path/to/instances/my-instance/.minecraft","world":"My World","file":"/tmp/my-world.zip"}}
{"protocol":1,"id":"w-9","method":"worlds.import","params":{"directory":"/path/to/instances/my-instance/.minecraft","file":"/tmp/my-world.zip","name":"Imported","replace":false}}
{"protocol":1,"id":"w-10","method":"worlds.backup","params":{"directory":"/path/to/instances/my-instance/.minecraft","world":"My World","label":"pre boss"}}
{"protocol":1,"id":"w-11","method":"worlds.backups","params":{"directory":"/path/to/instances/my-instance/.minecraft"}}
{"protocol":1,"id":"w-12","method":"worlds.restore","params":{"directory":"/path/to/instances/my-instance/.minecraft","backup":"My World-20240102-030405-pre-boss.zip","replace":true}}
{"protocol":1,"id":"w-13","method":"worlds.backups.delete","params":{"directory":"/path/to/instances/my-instance/.minecraft","backup":"My World-20240102-030405-pre-boss.zip"}}
```

`worlds.export` returns `{file, size_bytes, files}`. `worlds.import` and
`worlds.restore` return the resulting `world` entry. `worlds.backup` and
`worlds.backups.delete` return `{"backup":"<file>"}` / `{"deleted":"<file>"}`
and `worlds.backups` returns `{backups:[{file, world, created_ms,
size_bytes}]}`.

`server.worlds.list` resolves `level-name` from `server.properties` and lists
the base world plus existing `_nether`/`_the_end` siblings; dimension
directories have no `level.dat`, so their metadata fields are `null`.

Server mutations and archives (all require a stopped server):

```json
{"protocol":1,"id":"w-14","method":"server.worlds.rename","params":{"directory":"/path/to/servers","id":"my-server","world":"world","new_name":"survival"}}
{"protocol":1,"id":"w-15","method":"server.worlds.delete","params":{"directory":"/path/to/servers","id":"my-server","world":"world_the_end"}}
{"protocol":1,"id":"w-16","method":"server.worlds.backup","params":{"directory":"/path/to/servers","id":"my-server","label":"full"}}
{"protocol":1,"id":"w-17","method":"server.worlds.backups","params":{"directory":"/path/to/servers","id":"my-server"}}
{"protocol":1,"id":"w-18","method":"server.worlds.restore","params":{"directory":"/path/to/servers","id":"my-server","backup":"world-20240102-030405-full.zip","replace":true}}
{"protocol":1,"id":"w-19","method":"server.worlds.backups.delete","params":{"directory":"/path/to/servers","id":"my-server","backup":"world-20240102-030405-full.zip"}}
```

Error codes: `WORLD_NOT_FOUND`, `WORLD_EXISTS`, `WORLD_LOCKED`,
`WORLD_INVALID_LEVEL_DATA`, `WORLD_ICON_TOO_LARGE`, `WORLD_LIST_FAILED`,
`WORLD_GET_FAILED`, `WORLD_RENAME_FAILED`, `WORLD_COPY_FAILED`,
`WORLD_DELETE_FAILED`, `BACKUP_NOT_FOUND`, `WORLD_ARCHIVE_TOO_LARGE`,
`WORLD_ARCHIVE_INVALID`, `WORLD_EXPORT_FAILED`, `WORLD_IMPORT_FAILED`,
`WORLD_BACKUP_FAILED`, `WORLD_RESTORE_FAILED`; server mutations add
`SERVER_ALREADY_RUNNING`; invalid names and out-of-range input use
`INVALID_PARAMS`.

## CLI

```sh
jmcl-core instance worlds list ID [--instances-directory PATH] [--json]
jmcl-core instance worlds show ID WORLD [--instances-directory PATH] [--json]
jmcl-core instance worlds rename ID WORLD NEW_NAME [--instances-directory PATH]
jmcl-core instance worlds duplicate ID WORLD NEW_NAME [--instances-directory PATH]
jmcl-core instance worlds delete ID WORLD [--instances-directory PATH]

jmcl-core instance worlds export ID WORLD FILE [--instances-directory PATH]
jmcl-core instance worlds import ID FILE [--name NAME] [--replace] [--instances-directory PATH]
jmcl-core instance worlds backup ID WORLD [--label LABEL] [--instances-directory PATH]
jmcl-core instance worlds backups ID [--instances-directory PATH] [--json]
jmcl-core instance worlds restore ID BACKUP [--name NAME] [--replace] [--instances-directory PATH]
jmcl-core instance worlds backups-delete ID BACKUP [--instances-directory PATH]

jmcl-core server worlds list ID [--servers-directory PATH] [--json]
jmcl-core server worlds show ID WORLD [--servers-directory PATH] [--json]
jmcl-core server worlds rename ID WORLD NEW_NAME [--servers-directory PATH]
jmcl-core server worlds delete ID WORLD [--servers-directory PATH]
jmcl-core server worlds backup ID [WORLD] [--label LABEL] [--servers-directory PATH]
jmcl-core server worlds backups ID [--servers-directory PATH] [--json]
jmcl-core server worlds restore ID BACKUP [--replace] [--servers-directory PATH]
jmcl-core server worlds backups-delete ID BACKUP [--servers-directory PATH]
```

`instance.delete` removes the whole instance tree, so instance worlds need no
registry entries, no GC rules, and no store interaction.

## Out of scope

In-game (live) export while a world is open, incremental backups, backup
retention policies, and RCON-driven save flushing before backup are not
implemented; back up stopped servers/quiescent worlds only.

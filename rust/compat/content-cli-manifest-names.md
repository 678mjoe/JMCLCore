# Local content CLI: validate unused display-name bytes

Ordinary instance `mods`, `resourcepacks`, and `shaderpacks`
`list/enable/disable/remove` load `instance.json` before opening the game
content root. Pinned Zig `src/cli/content.zig` discards this document after
validation; its display name is a typed byte string. Rust previously used
its operational String manifest here and rejected legitimate byte arrays
before reaching an otherwise valid local content operation.

The production change is one call in `content/cli.rs::local_inner`:
`instances.load_read(id)` instead of `instances.load(id)`. The returned
value is still discarded. Both methods use the same anchored `load_with`,
read limits, alias policy, requested-ID check and ordered manifest validation.
No shared Root/Manifest API, operational load, installation identity, adopt,
server or acquisition path changes. Datapacks continue to bypass instance
manifest loading, exactly as `src/cli/datapacks.zig` does.

```sh
python rust/compat/content_cli_manifest_names.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/checkout-at-5a4310d
```

Linux evidence is 21 manifest cases × 4 families × 4 operations = **336
exact command comparisons**. Plain String and Unicode controls, ASCII,
Unicode, opaque/control and typed-u8 arrays, 256/257-byte boundaries,
empty/malformed input and stored-ID/schema/source/duplicate precedence have
explicit source-expected outcomes. Every valid fixture lists, disables,
enables and removes one inert registered file with independently asserted
registry/file transitions. Every invalid ordinary manifest preserves the
whole tree; all 21 datapack cases remain positive bypass controls.

Exact stdout/stderr/status and disk contents/modes/link counts are compared.
Manifest and readonly hardlink peer/sibling bytes, inode identity, mtime and
permissions are preserved; link count changes only for the one successful
removal. Failures collect both engines' per-invocation raw snapshots and
outputs before failing strictly. Windows readonly fixtures are retained,
never chmod-cleaned. No equal-error result can stand in for a positive case.

The previous frozen binary fails the ASCII-array positive case with
`failed to load instance: UnexpectedToken`; the patched binary passes all
336 comparisons and the existing 400 native-path comparisons. Native
macOS/Windows execution remains pending CI; there are no readonly-error
allowances. This is a discarded-metadata consumer fix, not broader operational
manifest byte support or complete CLI parity. No executable fixtures, Java,
provider calls, acquisition, archive operations or real game data are used.

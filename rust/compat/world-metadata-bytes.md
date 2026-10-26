# Read-only world metadata byte strings

Pinned baseline: Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

The NBT parser copies tag-string bytes without validating UTF-8
(`src/worlds/nbt.zig`, `parsePayload`, `tag_string`). `LevelMeta` stores
`level_name` and `version_name` as optional byte slices; `worlds.list/get`
serialize those values directly and compare nonempty version slices by bytes.
Previously the Rust read view called `Tag::text`, silently changing opaque
metadata into null and its version relation into unknown.

The private `worlds/read.rs` view now retains the original NBT bytes for those
two fields. Valid UTF-8 remains a JSON string, opaque bytes become a JSON byte
array, and absent/wrong-type fields remain null. Version comparison uses the
original bytes. Shared NBT decoding, numeric/boolean metadata, operational
String APIs, CLI, mutation/archive methods, alias flags and lock behavior are
unchanged. This is not a claim that those other consumers preserve opaque
metadata.

Run the bounded corpus with native binaries:

```sh
python rust/compat/world_metadata_bytes.py --zig /path/to/pinned/jmcl-core \
  --rust /path/to/jmcl-core-rust --oracle-source /path/to/pinned/checkout
```

Thirty exact list/get responses cover ASCII, Unicode, embedded NUL/control
bytes, opaque metadata, same/different/empty versions, empty and missing
strings, wrong tags/compounds and duplicate tags (first source match wins).
Positive world/game-mode, size/icon/lock fields and baseline metadata are
asserted, not merely matching errors. Whole fixture bytes, modes, mtimes,
inodes and link counts are preserved, including an inert hardlinked peer.
Owned fixtures are retained unchanged on every platform; no cleanup candidate
names or executable files are created. No world mutation, archive operation,
Java or network access occurs.

Linux red/green evidence: the frozen pre-fix binary differed on ten responses;
the fixed binary matched all thirty. The existing 144-request read corpus and
six enumeration/count scenarios also pass. Native Windows/macOS execution of
this new corpus remains pending the exact-tree CI gate.

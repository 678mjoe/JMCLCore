# Instance world CLI read view

Only `instance worlds list/show` now reuse the private read view. The shared
operational `world`, NBT parser, manifest lookup, CLI argument parser and every
mutation/archive branch remain unchanged. Show still reads metadata without
reading the icon payload, as before. Public operational String APIs are not
changed.

Pinned Zig `5a4310de` writes NBT display fields as raw `{s}` bytes in text mode
(`src/cli/worlds.zig`, `printWorldRow` / `printWorldDetail`). JSON emits valid
UTF-8 strings or opaque byte arrays. The three displayed strings—directory
name, level name and version name—now follow that rule; null remains `-` and
empty remains empty. Checked array output cannot panic on invalid values.
Shared server formatters retain identical String/null behavior; their lookup
paths are not changed or exercised by this slice.

List explicitly reuses the already verified byte-name enumeration, source
count-before-inspection and final byte-sort behavior. It does not broaden
native OS argument decoding. Opaque directory names may appear in listing
output; this is not evidence that every CLI argv form accepts them.

Run with native binaries and the pinned source checkout:

```sh
python rust/compat/world_cli_metadata.py --zig /path/to/zig-core \
  --rust /path/to/rust-core --oracle-source /path/to/pinned/source
python rust/compat/world_cli_list_names.py --zig /path/to/zig-core \
  --rust /path/to/rust-core --oracle-source /path/to/pinned/source
```

Linux evidence: 60 exact metadata commands (15 fixtures × list/show × text/JSON)
and 12 enumeration commands (six scenarios × text/JSON) pass. The old binary
failed opaque metadata and opaque listing controls. Metadata fixtures use an
ordinary valid manifest with version `1.20`; relations are asserted against
that actual version, not an unused RPC parameter. ASCII, Unicode, NUL/control,
opaque, empty, missing/wrong-tag and first-duplicate values are covered.
Enumeration checks positive raw output, malformed metadata fallback and the
1024/1025 limit. All fixture bytes, modes, mtimes, inode identities and link
counts remain unchanged. Readonly peers are never chmod-cleaned; fixtures are
retained. No cleanup-candidate names, server calls, mutations, archives, Java,
network or game execution occur.

Native Windows/macOS results remain pending exact-tree CI. Physical raw-name
success is capability-dependent (APFS EILSEQ / Windows invalid-name); Unicode
controls remain mandatory and exclusions are reported explicitly, not claimed
as raw-name success. Existing RPC metadata/parameter/enumeration regressions
remain independent gates.

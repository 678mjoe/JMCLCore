# Persisted display names in read-only world consumers

Pinned source: Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
Instance/server manifests use typed byte strings for their human display name.
The existing Rust `ReadManifest` preserves that field and its typed-u8 grammar;
operational `Manifest` retains its String API. World read consumers previously
used the latter even though they never consume the display name, causing valid
ASCII/Unicode/opaque arrays to fail and changing semantic error precedence.

Only three read boundaries change:

- Instance CLI `worlds list/show` validates with `load_read`, then uses its real
  String `version_id`. Other commands keep operational loading. No placeholder
  name/version or loader value is fabricated.
- Linux server RPC `server.worlds.list/get` selects `store::load_read` and
  discards the validated record. Other methods keep `store::load`.
- Portable server CLI list/show uses a private parser callback around the same
  anchored read. Its existing operational wrapper and non-read callers retain
  `Manifest::parse`.

Root handling, aliases, platform gates, request decoding, NBT parsing and all
shared schema validation remain unchanged. This does not widen persisted
version/loader fields or OS arguments and does not establish general manifest
byte-string parity.

```sh
python rust/compat/world_read_manifest_names.py --zig /path/to/zig-core \
  --rust /path/to/rust-core --oracle-source /path/to/pinned/source
```

The bounded corpus contains 210 exact commands: instance CLI list/show and
server CLI list/show text/JSON plus server read RPC. It tests String, ASCII,
Unicode, opaque and legacy quoted-u8 arrays; 256/257-byte and empty boundaries;
wrong types, null, fractional/overflow bytes; and inverse ID/schema/name/version
validation conflicts. Valid fixtures have actual NBT version `1.20`; positive
responses must preserve that version and the real instance comparison result.
Malformed fixtures assert errors rather than accepting matching failures as
positive evidence. Every preexisting file, mode, mtime, inode and link count,
including readonly peers, is preserved. Fixtures have no PID/process state,
executable or cleanup candidate names. No server lifecycle, world mutation,
archive, Java or network operations run. Non-Linux RPC platform rejection is
asserted independently of portable CLI success.

Linux red/green: 134 of the 210 comparisons differed before the consumer fix;
all 210 now match. The existing 60 instance CLI metadata, 138 server metadata
and 28 server root commands also pass, as do six read-view units, one pure
formatter test and library Clippy with warnings denied. Native Windows/macOS
execution of this new corpus remains pending the exact-tree CI gate.

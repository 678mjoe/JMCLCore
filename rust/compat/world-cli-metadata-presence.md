# Server world show metadata presence

Pinned `src/worlds/worlds.zig:100–124` parses a level.dat once and represents
metadata as present when the root `Data` tag is a compound, even if every
recognized field is absent or has the wrong type. `src/cli/worlds.zig:96–114`
then prints detailed metadata or `level.dat: unavailable` using that presence
flag. Explicitly selected world and level.dat aliases are followed by the
existing metadata read.

Rust's server text show paths previously repeated a no-follow world lookup
only to decide that formatting flag. Thus a named world alias could return
correct JSON metadata but misleading `level.dat: unavailable` text. A bounded
88-command Linux proof found four differences: ordinary/raw-argv text show
for world aliases and combined world/level aliases. Instance show and JSON
controls matched; all owned alias targets and readonly peers were unchanged.

A private `world_read_with_metadata` returns the existing JSON value together
with the exact Data-compound presence from the same parsed document. The
existing `world_read` API simply returns the value, with no new wire fields.
Only the two server show consumers use the flag; the extra metadata file read
is removed there. Instance show, filesystem flags, NBT parsing, resource limits,
JSON serialization, recursive traversal and all mutation/archive routes are
unchanged. The flag is not inferred from nullable display fields.

The `world_cli_metadata_presence.py` corpus checks text/JSON and ordinary/raw
argv against pinned Zig using 13 owned fixture shapes for each instance/server
route. Empty Data and all-null fields still require the metadata section;
missing, broken, directory, malformed or noncompound Data require unavailable.
Broken/file world targets preserve earlier errors. Full bytes, modes, mtimes,
inode/link identities and outside alias peers are checked after every command.
The fixed Linux binary passes all 104 commands (the expanded baseline had
four differences), plus 180 native-argv, 138 server metadata and 60 instance
metadata regression commands. Static library/bin Clippy passes. No broad
world test filter is required or used. Fixtures are retained without chmod cleanup. Windows lacking symlink privilege
reports those physical cases as excluded, not passed; ordinary controls remain
required. Exact native results are separate from local Linux evidence.

```sh
python rust/compat/world_cli_metadata_presence.py --zig "$JMCL_ZIG_ORACLE" \
  --rust target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
```

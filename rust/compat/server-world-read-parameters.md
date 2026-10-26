# Server world read parameters

Pinned Zig `5a4310de` defines `ServerWorldsListParams` and
`ServerWorldsTargetParams` with dynamic byte strings for directory/id/world and
optional version. The previous shared Rust Value/String decoder rejected valid
ASCII arrays and ignored large numeric fields. A private early list/get branch
now preserves the selected raw lexemes using the existing dynamic-u8 decoder.

The outer lexical/duplicate guard and non-Linux unsupported gate remain before
this module. An explicit object guard rejects serde positional structs. Missing
directory retains the source `servers` default. Empty checks precede filesystem
access; native directory conversion and root opening precede semantic ID
validation. Only then is a valid ASCII ID used with the existing anchored
manifest loader. World/version bytes are not lossily converted; comparison uses
the actual requested bytes. Property-derived three-name ordering is unchanged.
The prior mutation/archive decoder, CLI, shared loader and lifecycle code are
unchanged.

```sh
python rust/compat/server_world_read_params.py --zig /path/to/zig-core \
  --rust /path/to/rust-core --oracle-source /path/to/pinned/source
```

Linux red/green: 81 exact requests, 36 previous differences, now zero. Cases
include valid arrays, binary64 raw versus quoted numeric precision, legacy
quoted integer grammar, empty/overflow/wrong types, opaque IDs/version bytes,
root-before-ID/name errors, ignored `1e400`, malformed lexical strings,
duplicates and non-object parameters. Positive version metadata and relations
are asserted. Physical opaque root/world lookups use owned preexisting paths;
unsupported EILSEQ outcomes are explicitly reported and compared as rejection,
not counted as physical success. Other platforms assert the Linux-only RPC
gate and do not claim physical server lookup coverage. All fixture bytes,
permissions, mtimes, inode/link identities and readonly peers are preserved.
No PID/process-state, executable or cleanup-candidate files are created, and
no lifecycle, archive, Java, game or network operation runs.

The separate named-world/icon alias boundary is now covered by
[server-world-icon-aliases.md](server-world-icon-aliases.md). That get-only
followup does not change parameter decoding or the mutation decoder.

Local focused tests and exact native CI are distinct. Native execution of this
new corpus remains pending the exact-tree CI gate.

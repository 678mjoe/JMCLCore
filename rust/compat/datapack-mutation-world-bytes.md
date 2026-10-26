# Guarded datapack mutation world bytes

Pinned source `5a4310debbcee26b3d2391bab2339f79cd6ec52e` accepts dynamic byte
world names for local enable/disable/remove. Its mutable opener acquires game and
saves, validates the world name, opens that existing child, then probes the world
lock before selector validation and registry operations.

Byte-world inputs now follow the same order through a private explicit
`mutable=true` helper. LIST retains its read-only wrapper, permanently passing
false. Ordinary mutable String inputs still take the original String helper;
all public String world helpers, guard internals, filesystem/store code and
adoption serializer wrapper remain unchanged. Only the local world byte parameter
boundary broadens, not server properties, CLI arguments, installation or adoption.

The old mutation-array rejection test now asserts CONTENT_NOT_FOUND in an owned
existing empty world with no created files. It was red against the prior tree.
The executable oracle corpus covers 90 paired fresh sequences /180 RPC pairs on
Linux, reducing48 differing sequences to zero. It checks String, ASCII/native
arrays and dynamic numbers, invalid/empty/missing world, blocked root, selector
range/semantics, held owned lock ordering, repeated operations and absent physical
files. Explicit positive expectations cover exact world String/byte output,
registry state, renamed/removed paths, and immutable shared peer inode/mode/mtime/
bytes (with only expected link-count changes). Failed calls preserve full snapshots.

```
cargo test --test datapack_list_world_bytes
python rust/compat/datapack_mutation_world_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

Ordinary/error cases run all OS. Held flock controls are Unix-only: Windows lock
coverage is explicitly not established here. Raw FF (Unix) /WTF-8 (Windows) world
creation probes exclude only exact EILSEQ/ERROR_INVALID_NAME capability failures;
other errors fail. Both engine records are retained before strict contract
failures. Every mismatch prints full normalized state plus separate per-engine
peer diagnostics; equal source/Rust readonly failure is not accepted as a positive
pass. Windows owned readonly fixtures are retained and printed rather than clearing
shared attributes for cleanup. Local green evidence is Linux, not native acceptance.

All payloads are inert opaque bytes with registry-required suffixes. No archive
parsing, provider requests, CAS acquisition, adoption, Java, PIDs or signals run.

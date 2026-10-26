# Datapack LIST world byte names

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e` accepts dynamic byte strings
for `DatapackWorldParams.world` (protocol/datapacks.zig). Service
`listWorldDatapacks` opens the game/saves directory before calling
`worlds.openWorld`; the latter validates the exact name bytes then opens the
existing child. Listing does not inspect locks or invoke world cleanup.

The local parameter type now has a LIST-only byte-name variant. Mutable world
parameters remain String. An additive private Root read helper acquires saves,
validates the same 255-byte/leading-dot/trailing-space-dot/separator/control rules,
and uses the existing native byte conversion. The only filesystem change is a
six-line outer `read_child_bytes` wrapper permanently passing `create=false` to
the existing private child method; its visibility, flags and alias semantics are
not changed. Existing public/mutable world helpers and String validator are
byte-identical. No store, server, CLI, adoption or acquisition decoder changes.

The existing String `result(stage, Option<&str>, fields)` wrapper remains intact
for adoption callers, delegating to a generic serializer. LIST uses the existing
ListedPath representation: valid UTF-8 String, otherwise numeric byte array.
Pure tests exercise the old String/adoption result shape without running adoption.

Validation:

```
cargo test --test datapack_list_world_bytes
cargo test --lib content::tests::string_world_result_wrapper_remains_unchanged -- --exact
python rust/compat/datapack_list_world_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

The prior tree fails the new ASCII world-array unit with INVALID_PARAMS and
differs on 176 of 368 paired read-only cases. The corpus checks positive exact
world output/unmanaged content, String/ASCII/Unicode/native arrays, dynamic numeric
grammar, unsafe/empty/long/missing/non-directory world names, missing or non-directory
saves, and blocked game roots. Complete snapshots include sibling hardlink identities.
Only exact physical-name EILSEQ (Unix) or Windows ERROR_INVALID_NAME is reported
as unsupported capability; other errors fail. This worker observed Linux raw FF
success; cross-platform preparation is not native Windows/macOS evidence.

All new world fixtures have existing game roots; they do not permit core-created
roots or worlds. The prior separate client-directory corpus retains its explicit
15 owned empty-root creation/range controls to verify unchanged root semantics.
No locks, mutation/adoption execution, archive parsing, Java or requests are used.

The later local mutation-byte extension is documented separately in
`datapack-mutation-world-bytes.md`; the original LIST stage above remains its
read-only evidence boundary.

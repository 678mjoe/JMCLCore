# Server content LIST target bytes (separate prerequisite-dependent slice)

This slice is based on combined head `663b069e0b86cf45365f73faed976a5967f61735`
(tree `e20f99003a3028f0b694208e21cbd7111d72dc99`). It depends on both the content
and existing read/native-directory helper chains; it is not standalone against
an older content-only branch.

Only `server.mods.list` and `server.datapacks.list` RPC target decoding changes.
The existing Linux gate remains before parameter parsing. A list-only branch
uses existing `instances::lookup_target` dynamic-u8 decoding and
`servers::store::open_root_native`, then validates the ID and reuses the existing
manifest/open-server logic. Non-list Params parsing remains String-based.
Lifecycle/store source files are byte-identical. No guard, state, PID, signal,
process operation, metadata acquisition or artifact store is reached by LIST.

The same `world(..., false)` and content list-fields helper preserve datapack
properties/base-world behavior without creating a world. The ordinary manifest
loader is intentionally unchanged in this slice; display-name byte retention
for this particular consumer is a separate question, not implicitly fixed.

## Source / evidence

Pinned Zig core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/protocol/server_content.zig`, `Service.openServerContentDirectory`,
`listServerContent`, and the server datapack list path establish dynamic byte
parameters, range checks, root-before-ID validation, manifest ordering and the
absence of a lifecycle lock. Root failures and invalid/missing server IDs map to
SERVER_NOT_FOUND; other manifest/properties errors retain the existing mapping.

Baseline binary SHA256:
`a08196daf3dd62f945d29fa55762a6f3d8d6b53b997ab3dcc308827cd17a1ecd`.
The actual handler array-target unit was red. Green binary SHA256:
`2ea3e7910f55ffd92b10b4a26c58f50cc2abda84f0e19d8e28c79f1c5cf8ba28`.

- 2 Linux units pass (array acceptance and direct null diagnostic); the gated
  non-Linux unit checks UNSUPPORTED_PLATFORM before parameter decoding.
- 212 exact paired LIST RPC cases pass: normal Strings/arrays, Unicode/physical
  Linux raw directory, malformed/rounded/quoted dynamic elements, range/root/
  requested-vs-stored ID/manifest ordering, ignored fields, duplicate and missing
  parameters, missing world and invalid properties.
- Full fixture snapshots check bytes/device/inode/mode/mtime/link count and sibling
  hardlinks after every invocation. All roots already exist; no state files or
  PIDs are provided. No root creation is claimed by this corpus.
- Existing server opaque listing and registry-byte listing tests pass; formatting,
  all-target Clippy and diff checks pass. No native non-Linux filesystem acceptance
  claim is made; the portable script checks only the platform gate there.

```sh
cargo test --manifest-path rust/Cargo.toml --test server_content_list_target_bytes
python3 rust/compat/server_content_list_target_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

A unique low-debug target bounds disk use. No CLI decoder, non-list mutation,
server lifecycle, Java, archive parsing, network or remote write is included.

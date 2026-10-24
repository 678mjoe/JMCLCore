# Content provider service (injected vertical slice)

Native Rust Modrinth schema validation, stable-release selection, primary-file
selection, required dependency traversal and registry skip rules are available in
`content::modrinth` and `content::remote`. `Root::install_resolved` executes verified
CAS downloads and publishes registry entries only after all planned files finish.
`Root::set_modrinth_version` preserves disabled state, replaces the registry before
removing the old filename, and returns a no-op without metadata fetch for the same
version. These APIs use injected metadata/artifact transports; no new remote RPC
or CLI claims are made by this slice.

The adapter `content_metadata_oracle.zig` imports unmodified pinned Zig
5a4310debbcee26b3d2391bab2339f79cd6ec52e and compares normalized typed metadata
or exact error names. Existing JSON byte-array strings and underscore numeric
coercion are deliberately retained, rather than rejecting all unusual inputs.

Tests additionally execute real hardlinks, transfer failure without registry
publication, installed-project skipping, dependency ordering/cycle suppression,
disabled version replacement and no-network same-version no-op.

Remaining gates: full service/RPC/CLI oracle with fixture transport; CurseForge
resolution; production bounded transport and streaming dispatch; native Windows
execution. Non-UTF8 metadata byte-array strings currently reject rather than
retaining arbitrary bytes, and extreme f128 numeric coercion remains a shared
known gap. This is not full content compatibility.

# Content provider service (injected vertical slice)

Native Rust Modrinth schema validation, stable-release selection, primary-file
selection, required dependency traversal and registry skip rules are available in
`content::modrinth` and `content::remote`. `Root::install_resolved` executes verified
CAS downloads and publishes registry entries only after all planned files finish.
`Root::set_version` preserves disabled state, replaces the registry before
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

## Install wire follow-up

`content::rpc::handle(method, params, id, out)` now owns the four `.install`
methods and must precede generic Value handlers. `handle_with` injects metadata
and artifact transports for tests. The writer stays on its owning thread; the
shared bounded rendezvous adapter acknowledges each event before work continues.
A writer failure is tested to prevent registry publication. Request keys use
borrowed raw JSON and a wiping decoded string, never a generic Value.

CurseForge now resolves paginated files and required dependencies, chooses newest
available stable files, and falls back to the download-URL endpoint. Metadata
gets the borrowed key; the artifact transport never does. HTTPS transport is
bounded, but live networking is not acceptance-tested. Keyed cross-origin
redirects are not followed; this is an explicit transport compatibility boundary.

Evidence: 52 raw pre-network error responses against the **unmodified** pinned
binary; 32 raw event/result plus disk comparisons using the **injected service
oracle**, four kinds × two providers × success/skip/bad id/missing fixture.
The latter copies pinned source and replaces only `fetchBodyWithHeader` with a
local URL/body map. `build_content_fixture_oracle.py` verifies source HEAD/clean
source and records original/modified hashes plus the exact patch. It does not
modify the pinned checkout and is not described as an unmodified wire oracle.
Artifact data is pre-verified in the real CAS; any unexpected network attempt
fails the synthetic Rust transport.

Remaining: set-version external dispatch and CF variant; CLI install/switch;
non-UTF8 metadata and full malformed schema envelope parity; native platforms.


## Version switching and CLI follow-up

All four `.set-version` handlers and native install/set-version CLI paths now
execute both providers. The streaming entry above owns eight remote methods;
existing `content::cli` forwards install/set-version itself (no extra main edit).
`Root::set_version` replaces the earlier Modrinth-only injected API and takes an
optional borrowed key. A same-version no-op requires no key/network even for CF,
matching the original ordering. Install resolution and registry publication use
the same loaded registry snapshot, not a silent second read during metadata I/O.

Current explicit Linux corpus (all assertions executed, no oracle skips):
- 120 adoption RPC + 28 adoption CLI raw/disk cases against unmodified pinned Zig.
- 60 install pre-network raw error cases against the same unmodified binary.
- 83 pure metadata typed-result/error comparisons against pinned source adapter.
- 32 install + 40 switch raw event/result and disk cases against the injected
  service oracle; enabled/disabled/no-op/not-found/incompatible switch cases.
- 96 raw CLI/status/stderr and disk comparisons against the injected oracle,
  including real filename-changing version switches, not just no-op success.
- Actual CAS inode/readonly checks, failed transfer without registry publication,
  dependency ordering/cycle suppression, key separation and writer failure.

The service oracle builder now compiles ReleaseSafe, matching the pinned CI
optimization; Debug error-return traces are not normalized away. Output paths
inside/equal/containing pinned source are rejected before copying. `.exe` and an
optional shared Zig global cache are supported. Artifact provenance includes
optimization. Header copies made internally by reqwest are outside the typed
wiping ownership guarantee; metadata is byte-bounded, not newly time-bounded.

Not covered by these counts: real TLS/redirect/error behavior, native Windows/mac
execution, complete malformed nested CF envelopes, arbitrary non-UTF8 metadata
byte strings, extreme f128 number coercion. These remain visible compatibility
gates rather than being called complete merely because handlers exist.


## Provider schema audit

A further 115 pinned CurseForge file-schema cases now match exact normalized
results/error names. Content owns its typed response decoder instead of silently
inheriting a narrower modpack projection: quoted underscore IDs and signed zero
retain Zig integer coercion. Project and nullable download-URL responses use the
same typed path. A separate local regression ensures a valid files page can
contain an ignored field larger than 2 MiB while remaining inside its 8 MiB page
limit; it must not accidentally inherit the standalone-file envelope limit.
The service builder also records the completed binary hash in provenance.
Arbitrary non-UTF8 byte strings and extreme f128 coercion remain explicit gaps.

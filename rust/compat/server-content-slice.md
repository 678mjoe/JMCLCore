# Server mods and base-world datapacks

Pinned source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

`server_content::handle` owns the fourteen `server.mods.*` and
`server.datapacks.*` methods (list/install/set-version/enable/disable/remove/adopt).
Call it before generic Value-only dispatch because installation streams events.
`server_content::cli` accepts the arguments after `server` and returns `None`
for other server families. `handle_with` and `cli_with` inject provider metadata
and artifact transports for synthetic verification. No new wire/disk schema.

Both routes open/validate the server manifest, derive game/loader from that
manifest, and use descriptor-anchored content roots. All mutations hold the
existing `lock_stopped` guard through metadata resolution, transfer, publication,
and registry updates. Only mods **install** requires a non-Vanilla server at
this facade boundary; cleanup/adoption are allowed on Vanilla. Version switching
retains the content engine's own loader checks, including their ordering.
Read-only listing does not take the lifecycle lock.

Datapacks use the usable `server.properties` level-name and that base world's
`.jmcl/content.json`, not the server-root registry or dimension siblings. Listing
before the world exists is empty and does not create it. Mutations create it,
including unsuccessful selector mutations, as the pinned implementation does.
There is no client-world session lock here; the server lifecycle lock is the
original authority. Mutable world state is never put in CAS. Immutable adopted
and installed artifacts use the existing guarded hardlink store transactions.

Linux gate precedes argument validation for both RPC and CLI. RPC parameters use
borrowed raw JSON plus owned wiping decoded API keys, not a generic Value holding
keys. CLI owns a wiping copy while original caller argv ownership remains outside
this module. CLI and RPC intentionally retain distinct validation/error ordering.

## Executed Linux evidence

`cargo +1.99.1 test --test server_content -- --include-ignored --nocapture`
with the two explicit oracle variables below ran all ten test functions:

- 126 raw parameter/target RPC response comparisons against the unmodified oracle.
- 40 raw registry mutation/adoption comparisons, including exact registry bytes,
  file/slug selection, Vanilla cleanup, and hardlink inode preservation.
- 24 level-name/property/loader-precedence comparisons against unmodified oracle.
- 140 CLI argument/target comparisons (status/stdout/stderr).
- 16 injected-provider install comparisons with exact wire/registry/artifact bytes.
- 32 provider switch sequences (16 CLI, 16 RPC); each also compares CLI install,
  text/JSON list and resulting registry. Includes both providers, renamed files,
  disabled entries, same-version no-op and missing CurseForge key.
- 14 provider-selector and malformed-key-before-registry ordering comparisons.
- Three non-oracle tests: base-world creation without dimension mutation; live
  stopped-state guard using only a harmless stdin-waiting shell identity fixture;
  writer failure preventing registry publication. No game/server/Java was launched.

`JMCL_ZIG_ORACLE` is the unmodified pinned binary. `JMCL_CONTENT_SERVICE_ORACLE`
is built by `build_content_fixture_oracle.py`; only provider HTTP metadata fetch
is replaced by a fixture URL/body map. The actual CAS is pre-populated with benign
bytes; unexpected Rust network attempts panic. Injected oracle provenance records
source head, ReleaseSafe, original/modified source and exact patch hashes. This is
not presented as a live-network or unmodified-network differential.

Library + this integration test pass strict clippy. Dispatcher wiring is included in the combined integration and tested locally;
this domain slice alone is not an executable acceptance claim. The slice inherits provider/transport/platform limitations in
`content-remote-slice.md`: real TLS/redirect/error behavior, malformed exotic
metadata numeric/non-UTF8 cases, and native Windows/mac execution remain gates.
Method presence and these synthetic cases do not establish full replacement parity.

Cross-target note: local Windows GNU checking did not reach Rust code because the
C dependency compiler/cache environment was incomplete. The optional combined cross-check was stopped before completion;
native Windows CI remains the acceptance gate.

Final oracle recheck used the dedicated `JMCLCore-rust-oracle` checkout (HEAD
exactly the pinned source), binary SHA-256
`b9e628efc5b7ea34047ad25129a98756b6f8ec5d0be38193aee2620d87a82bfe`.
All ten integration functions passed again. The separately maintained
`JMCLCore-integration` checkout is mutable and should not be treated as source
provenance for an existing binary. Final library regression: 148 passed, zero
failed, seven explicitly ignored oracle tests outside this slice. Strict clippy
for the library and server-content integration test passed.

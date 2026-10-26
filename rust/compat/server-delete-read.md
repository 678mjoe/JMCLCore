# Stopped-server deletion: metadata view and error order

Baseline source is Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`:
`src/protocol/server.zig::writeDeleteResult`,
`src/cli/server.zig::runServerDelete`, and
`src/server/server_store.zig::delete` establish this order:

1. Open the servers root and the requested server directory.
2. Acquire the existing stopped guard.
3. Load and validate the persisted manifest.
4. Delete the selected server tree.

Rust previously loaded the operational String manifest before the guard. This
rejected valid byte-array human names, returned load-phase errors during deletion,
and hid the original guard-before-manifest ordering. A delete-only branch now
uses the existing byte-preserving read manifest after the unchanged guard. It
never passes that view to launch/install consumers. Original errors are mapped at
the actual failing phase; the CLI receives the diagnostic from the same operation,
without re-reading a failed manifest. Other RPC branches and operational manifest
APIs remain unchanged.

## No-state test boundary

The fixture safety condition is explicit: no `.jmcl/server.pid`, PID/process-state
file or executable is supplied. Source `loadState` returns null on a missing
state file, and `statusUnlocked` returns stopped before `processMatches`. The
Rust `load_state`/`status_unlocked` path has the same ordering. `lock_stopped` and
all of `lifecycle.rs` are unchanged. The tests invoke only deletion, never
status/start/stop/restart or a Java entrypoint. This does not claim new running
server or process-identity coverage.

`server_delete_read.py` checks **56 exact RPC/CLI pairs** on Linux: normal and
ASCII/opaque/control-byte names, malformed/missing/duplicate manifests, invalid
names/IDs, requested-ID mismatch, missing targets, and a blocked guard path.
Independent assertions require valid records to be deleted, invalid records to
remain, guard failures to win over manifest errors, and exact phase diagnostics.
Both precreated guards and absent `.jmcl` are tested. When manifest validation
fails after a successful guard, the original creates exactly `.jmcl` and its
empty `lifecycle.lock` (0600); the corpus checks that effect rather than masking
it with fixture setup. Successful deletion removes those owned artifacts too.
Sibling bytes/metadata and external-to-target hardlink peers remain unchanged;
the shared link count drops by exactly one only when the target is deleted.

The baseline binary fails the new guard CLI diagnostic assertion; the prior
source-pinned proof also established valid opaque-name deletion mismatches.
The patched binary passes all 56 pairs. Existing **75** server show/get and
**458** manifest name read-view comparisons pass unchanged. All-target Clippy
passes. Native non-Linux runs remain pending: the script explicitly requires the
unchanged unsupported-platform response and complete preservation there, not a
claim that server deletion is supported on those hosts.

The separately scoped [root-open correction](server-root-open.md) covers the
existing-file `NotDir` diagnostic and unchanged nofollow directory-open flags.

```sh
python rust/compat/server_delete_read.py \
  --zig /path/to/pinned/jmcl-core --rust rust/target/debug/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

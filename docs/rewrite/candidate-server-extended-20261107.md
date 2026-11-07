# Server handshake and loopback follow-up, 2026-11-07

Two further **exact existing tests passed**, each 1 passed / 0 failed / 0 ignored
with 7 filtered out. This extends the earlier [server lifecycle slice](candidate-server-followup-20261107.md),
not the original no-real-Java evidence. Production source is unchanged.

## Subject and checks

- Tested local commit: `effef4043ce823891399431e8ebe7ae9a6661854`.
- Tested tree: `93ac3f10cfa0cb4b268fbbd1f43b5ae1342dfc01`.
- Prior preserved remote head: `e5380182472f4c92147f6f3ac84fcda7793dd375`,
  tree `12d406b588ed0b2683f996abe8d5b554580e2f83`.
- Only `rust/tests/server_lifecycle.rs` and its isolation document changed before
  execution. The test additions bound fixture socket waits and observe owned
  process cleanup; no production selection, transport, lifecycle or dependency
  behavior was changed.
- Linux x86_64 GNU, existing Rust/Cargo 1.99.1. Targeted offline/locked no-run
  compile and all-targets Clippy `-D warnings` passed on the exact test source
  before execution. Final clean integrated commit `5c6ab9015c83ecda688ef9930ce2728728469aaa`
  (tree `ed014cd634318228cb606a4297d5568de795c024`) then passed `cargo fmt
  --all -- --check`, offline/locked `--all-targets --no-run`, and all-targets
  Clippy `-D warnings`, each with recorded exit 0. The final evidence addition
  changes only retained logs/manifest and this explanation; all tested source
  subtrees remain identical.
- Test executable SHA-256:
  `ec4c15b566302b3f0b817ab2febc9e506045a80db43770f024563ad447b353c5`.
- Core executable remains
  `dad858d76f05cc4fad65814b03790a83e02e9244c5fa1d3155f882b89c1fd1f5`.

One explicit loopback capability preflight successfully bound an ephemeral TCP
listener/client and an ephemeral UDP socket on `127.0.0.1`, exchanging only
synthetic local bytes. There was one attempt per protocol, no refusal, retry,
port replacement after failure or environment change.

## What actually ran

1. `supervisor_handshake_failure_kills_child_and_cleans_state` directly starts
   the actual Core's hidden supervisor with an absolute owned shell-fixture
   path. `/dev/full` is the existing handshake-write failure injection, not a
   system setting change. The test records the known supervisor PID, waits for
   its failed exit, checks the removed state and scans for remaining owned
   server/fixture argv. No Java discovery occurs in this test. The race can be
   too short to observe the child, so no positive child-start observation is
   claimed. Source cleanup plus failed supervisor exit and the final live-argv
   scan are the bounded evidence.
2. `managed_rcon_and_query_use_property_derived_loopback_ports` uses the reviewed
   major-99 fixture. It executes the installed OpenJDK-21 version/property
   probes, but starts only the owned shell server. Actual Core RPC resolves
   ports from properties and talks to self-owned `127.0.0.1` TCP/UDP endpoints.
   Synthetic RCON authentication, the split command response and secret
   exclusion, and basic Query hostname/port output pass. The deliberately
   non-routable `server-ip` value is not used as a destination. This is not a
   real RCON server or full Query acceptance.

The bounded fixture accept/read/write logic and process-scan rules are in the
[isolation contract](../../rust/compat/server-lifecycle-fixture-20261107.md).
Both tests were independently reviewed before execution and run sequentially
with exact selectors and the minimal environment. The loopback test joins its
fixture threads and explicitly stops the server. A scan cannot silently skip
permission/read failures or truncated cmdlines; it preserves the owned directory
and fails instead. Both runs reported no live matching owned argv and empty
owned TMPDIR, with no manual signal or cleanup needed. This does not assert
immediate OS reaping of exited processes.

## Evidence and remaining limits

The [manifest and logs](../../rust/compat/evidence/server-extended-20261107/manifest.json)
retain the exact commands, reviewed runner, environment, source/executable hashes,
preflight and results. Execution-log SHA-256:
`e39ed690c850207a8fb9fc2d67fcf2b777d325c47c866189363a4a7222cc7c6b`.
Runner JSON framing is distinguished from preserved test output.

No Java acquisition/install/runtime archive, real game/server, real account,
provider/external service, Tauri, Windows/macOS native execution or full runtime
suite ran. No permission denial occurred. These finite checks do not establish
universal method compatibility or permit Rust replacement; see the
[current matrix](candidate-status-20261107.md) for the important remaining scope.

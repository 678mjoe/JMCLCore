# Native Session trace capture for frontend replay

```
python rust/compat/frontend_bridge/run_instance_chain.py \
  --scenario instance --frontend /path/to/pinned/frontend \
  --rust /path/to/verified/core --trace-dir /path/to/new/output-directory
```

The destination must not exist. Capture runs the exact instance test under the
unchanged, actively self-tested network guard. Output is saved only after the
one-test success gate and frozen-Core digest verification pass. No production,
frontend, guard, or CI behavior is changed. The harness remains local opt-in.

`raw.jsonl` contains captured Session API values, not hand-authored fixtures:

- `fixture`: the owned temporary root;
- `session_open` / `session_close`: successful lifecycle observations by the
  test harness; session numbers distinguish the actual new Core on reopen;
- `request`: increasing sequence, current session number, method, exact params,
  every callback SessionEvent serialized in order, and outcome;
- `fixture_action`: the harness's owned canary/receipt changes, explicitly
  separate from Core requests/events.

Request outcomes are `{kind:"result",data:<returned value>}` or
`{kind:"error",data:<serialized SessionError>}`. Both `event` and `diagnostic`
callback objects are captured before branching on success/failure. Errors do
not discard events. These are original API values, not a byte-for-byte Core
stdout capture; the native Session internally consumes terminal wire envelopes.
Implicit Session establishment handshakes are not separately intercepted.

`normalized.json` wraps the same ordered records with `schema_version:1`.
The only transformation recursively replaces the exact fixture-root substring
inside string values with `${FIXTURE_ROOT}`. Dictionary keys, request IDs,
random native-directory suffixes, counters, callback envelopes, outcomes and
record order remain unchanged. `provenance.json` records the root mapping,
raw/normalized SHA-256s, frontend/module hashes, frozen Core SHA-256, capture
source commit/file hashes, test-binary digest and successful guard/test status.

The trace includes blocked-instance installed=false, cached execute events,
prepare's real PREPARE_FAILED error, close/new-Core reopen with installed=true
get/list, then removal of only the owned canary and a real execute+prepare retry
that succeeds. It also includes removing one's owned receipt and a subsequent
installed=false read. The retry is executed, not a duplicated success record.
Prepare requests contain only id/directory/source, while execute uses source,
workers/retries, store and local Java policy as the typed frontend wrapper does.
No Java execution, real accounts, archive parsing or provider traffic occurs.

A frontend test consuming these files must call its evidence trace replay,
not a live frontend/native/Tauri end-to-end run. Replaying a recorded sequence
with deferred scheduling can test React state transitions, but does not prove
the original UI's wall-clock timing or native IPC wiring.

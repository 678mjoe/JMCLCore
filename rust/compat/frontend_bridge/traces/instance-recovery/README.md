# Captured native instance recovery trace

Captured by committed harness `a64592885862b7a064f25b8019c350478b83d49d`.
The complete guarded execution log is `capture.log`; hashes and transformation
rules are in `provenance.json`. Original Session API data are `raw.jsonl`;
frontend replay consumes `normalized.json`. None of the event/outcome values
were hand-authored. There are 37 records, including 28 actual requests.

Useful request sequence numbers:
- 13: initial blocked-instance cached execute, three real callbacks.
- 14: actual prepare failure, PREPARE_FAILED.
- 16: post-close ping, SESSION_CLOSED.
- 20: new Core instance.list before retry, all three installed=true.
- 21/22: actual blocked-instance retry execute/prepare after owned canary removal.
- 27: instance.list after one's owned receipt removal: one=false, others=true.

The guard's syscall/architecture/exec/FD/IPC self-tests and exact 1-test run
passed. The independently recomputed normalized tree exactly equals raw
records after the documented root-only replacement, and both stored SHA-256s
match. Python compile, rustfmt and diff checks passed. Default Clippy completed
with the same five unchanged frontend transport warnings; none in new capture
code. Strict warning-free status is not claimed.

Trace replay can test frontend state transitions using these recorded inputs.
It is not live Tauri/UI end-to-end evidence. Session lifecycle numbering assumes
this deliberately serial, single-active-session fixture, not concurrent tracing.

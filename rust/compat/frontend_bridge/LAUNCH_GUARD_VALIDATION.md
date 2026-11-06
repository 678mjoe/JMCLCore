# Verified native launch chain, 2026-11-06

Result: **one exact native launch test passed, none failed or ignored** under
the separately reviewed launch-only guard. This is a real unchanged frontend
Session → production Rust Core dispatcher → native child process chain. It is
not Tauri IPC/UI, a JVM/game, real authentication, archive preparation, runtime
acquisition, running-process cancellation or cross-platform acceptance.

## Review and execution gates

Independent static review completed before candidate execution. It checked BPF
branch offsets, all six 32-bit halves of recvfrom's constrained arguments, both
halves of the AF_UNIX socketpair domain, descriptor hygiene, mandatory syscall
denials, ABI rejection, startup/thread rules and runner ordering. Review fixed
one selftest false-positive risk: socket-stderr rejection must capture the
specific refusal diagnostic through the local peer, not merely observe a
nonzero exit. Reads are bounded by two seconds and 64 KiB. The filter itself
also requires Python isolated/no-site startup before FD cleanup.

Reviewed and executed source hashes:

- `launch_network_guard.py`:
  `2790712aaf757536329cc1ac87c73931f04bf1a1f6dc3f375f782db08423e719`
- `run_launch_chain.py`:
  `64dce4b23441bf3e458cd8077f341063dc1252bfcb88b69e19d0cd21dc3782be`
- `tests/fixtures/launch_spawn_gate.rs`:
  `77b2f1569434d5e0a7779e32f8dd6a66a7badddf9040dfccb9a5436679273767`

The initial passing source checkout was clean at
`3b6133f9b683a0ee44bd000bff990ff68add7d4a`. Verification ran in this order:

1. Standalone reviewed-guard self-tests passed: 18 denied syscalls, exact local
   pair recvfrom, six nonzero argument-half cases, five rejected pair domains,
   x32 and all bare numbers 512–547, wrong architecture, two exec boundaries,
   new-thread inheritance, refusal of existing siblings, all three socket
   stdio positions and extra-fd closure.
2. The unchanged original guard's separate regression self-tests passed.
3. The explicit runner rebuilt offline/locked and repeated candidate self-tests.
4. The missing-child gate asserted `ErrorKind::NotFound` and returned 0. The
   successful own-child gate asserted exit code 0 and returned 0. Neither is a
   Java executable. Both precede the first Core launch.
5. The exact test inventory contained one intended test; the full chain passed
   in 0.14 seconds. No guard, production source or fixture correction was made
   between review and this successful run.

No rejected environment operation was retried or circumvented. The earlier
strace OS denial remains a limitation; no live syscall trace is claimed.

## Observed Session sequence

The retained trace contains original native Session callback objects and
returned Result/SessionError values:

1. First real Core handshake; missing absolute native override returns
   `JAVA_NOT_FOUND`, emits no nonterminal events and leaves no native directory.
2. Same-session launch returns exit 7. One started, six stdout and two stderr
   callbacks precede its terminal result; native cleanup reports `removed`.
3. Ping succeeds; another launch returns exit 0 with the same event/cleanup
   contract and a fresh child PID.
4. Close twice; a subsequent ping returns `SESSION_CLOSED` with no events.
5. A second real Core child handshakes; launch returns exit 0, cleanup reports
   `removed`, following ping succeeds, then close completes.

Every successful launch asserts exact literal argv, synthetic credential
redaction, working directory and prepared-native directory presence inside the
child. The child writes an owned canary there; terminal assertions verify that
whole per-launch directory is gone and the child has been reaped. The synthetic
client artifact remains exactly `hello`; the runtime-store destination remains
absent. Explicit override/local policy is fixed for every execute request, so
the production resolver's override branch cannot fall back to candidate
discovery or runtime acquisition.

Base64 bytes are decoded and compared exactly, including a UTF-8 code point
split across 16-KiB records, a genuinely empty newline record, binary stderr,
CRLF handling and access/session secrets split across child writes. Each record
is at most 16 KiB. This verifies record bounds, not a limit on total frontend
retained history.

## Identity and preserved evidence

Production Core executable SHA-256:
`dad858d76f05cc4fad65814b03790a83e02e9244c5fa1d3155f882b89c1fd1f5`.
Its production source is unchanged from base
`a87c3afac372653acb0ca9bd1a92b0fc9a743a0d`.

Frontend modules were copied unchanged from
`c0107f1803c73398a99f8ddd27ed712c0ed2e1b4`; all three were verified byte-identical
to snapshot `90b9d5472e620255af740b55315e2cf75c1c9115` using local Git objects.
This says nothing about other frontend modules or later revisions.

The initial passing `traces/native-launch-verified-20261106/` contains:

- `raw.jsonl`: original Session API trace, SHA-256
  `64ed4cf23742132d828abb0a7c0398e5a00f756efa559c8a6b79070cf52aa005`;
- `normalized.json`: only recursive literal fixture-root substitution,
  SHA-256 `8a32f984be208a34ff4f8915f611865dad702136cc59083bf1ad33e62973af06`;
- `provenance.json`: exact source/module/binary hashes, executing guard, gate
  results, capture boundary and limitations;
- actual test output and independent/original-guard selftest logs.

Request parameters include only explicit synthetic auth. The trace is not a
byte-for-byte capture of Core stdout; Session strips terminal envelopes into
returned values. PIDs, duration, RSS, native-directory nonce, request IDs,
events and ordering are preserved in normalized data.

The historical blocked evidence and original guard/instance runner remain
byte-for-byte unchanged. The old failure is not rewritten into a pass.
No default CI lane or old pin enables this launch-only test. `--all-features`
does enable it and must not be confused with the default-off workflow.

## Final recapture matching the frontend request shape

The frontend replay seam identified two harmless request-shape differences in
the initial native fixture: its game directory was `root/game`, and it included
an explicit empty `java_paths`. Production TasksProvider/CoreSession instead
uses an instance-local `.minecraft` directory and omits `java_paths` when an
explicit override is present. The owned native fixture was updated to send
those actual fields, rather than rewriting a captured request during replay.

The final run used clean source
`b7b7b9abe5360054819ace0733599163feabe25f`. Guard, runner, spawn gate, benign
child source and production Core were unchanged. All guard self-tests and both
spawn gates ran again before Core; the exact launch test again passed
1/0/0 in 0.14 seconds. All original blocked and initial passing trace files
remain byte-for-byte preserved.

Final actual params are `directory: root/instances/demo/.minecraft`,
`store_directory: root/never-create-store`, explicit owned `java_override`,
`java_policy: local`, no `java_paths`, and the original synthetic auth/options.
No fixture adapter or additional value normalization is required for frontend
replay. This is request-shape alignment, not evidence that a mounted frontend
was part of the native run.

Use `traces/native-launch-frontend-shape-20261106/` for that final evidence:

- `raw.jsonl` SHA-256:
  `e87118512ff5a7d7da2fcad6652fb3f788108bf4f0abfed019ed572158a34e3c`;
- `normalized.json` SHA-256:
  `f117361e6a1222de3f169b706edb11fd937b8813a0d8b4d3a006e4815bec5c34`;
- `provenance.json`, exact test output and complete runner log retain the new
  source/test-binary identities and guard/spawn evidence.

The only normalized value remains the recursive literal fixture-root
replacement. No IDs, native nonces, PID, duration, RSS, event data or ordering
were changed. The traces remain Session API observations, not raw-wire bytes.

Final static checks also passed: Python compilation, Rust formatting, offline
locked harness/default-off builds, standalone fixture compilation with warnings
denied, and repository diff checks. Clippy passed with the same five existing
warnings in unchanged imported `transport.rs`; no new-test warning was emitted.
The scoped trace attribute preserves the harness's exact terminal blank line
in `test-output.txt`, rather than editing captured stdout to satisfy whitespace
checking.

## Security boundary

The allowance is only recvfrom with zero flags and NULL source pointers; it
does not establish locality by itself. Locality depends on descriptor
provenance: no inherited socket, socket() denied, AF_UNIX pairs only, and
descriptor-import/injection operations denied. Filters are installed in fresh
single-threaded Python processes and inherited thereafter. The tests provide
evidence of those properties, not proof of general confinement.

This remains a trusted, owned fixture with no outside helpers. It is not a
filesystem, device, privilege, resource or hostile-code sandbox; arbitrary
cooperating outside processes, writable process-memory paths, or pipe/file
relays remain outside the claim. See `LAUNCH_GUARD_DESIGN.md` for the detailed
threat boundaries and future-change requirements.

# COMPAT-CAS-001: deterministic stale-preflight GC recheck

This is a separate test of the retained Rust digest-lock mechanism, not evidence
of whole-disk Zig parity or universal concurrent safety. The accepted persistent
lock-layout difference remains visible in adoption's exact per-engine fixtures.

The only injection is a private `#[cfg(test)]` callback on one collector `Store`
instance, initialized to `None` by both constructors. There is no global hook.
It runs after initial eligibility and dry-run handling, immediately before the
digest lock (on Windows after the existing file-handle drop). Non-test builds
contain neither callback storage nor invocation. Production lock and GC policy,
no-follow handling, hardlink publication and fallback behavior are unchanged.

The named ignored test runs the following owned, same-volume opaque-file schedule:

1. Import and unlink the owned source, leaving one readonly CAS link. Acquire its
   transaction. A separately opened descriptor must report specifically
   `WouldBlock`, not an arbitrary error.
2. A collector using a separate Store handle proves the one-link object eligible,
   signals arrival at the callback, and waits. The transaction materializes an
   owned peer, then is dropped before the callback resumes.
3. The resumed collector must recheck under its lock: exact summary is one scanned,
   one linked, zero deletions/errors/busy/other counters. The CAS and peer retain
   identical original inode/file identity, readonly state, bytes and SHA1.
4. A descriptor can now acquire/release the lock. After unlinking only the owned
   peer, a subsequent GC must delete exactly the one eligible 17-byte object,
   retaining the same empty regular lock leaf.

This demonstrates a stale eligibility decision being invalidated by post-lock
rechecking, plus separate descriptor exclusion/release. It does **not** demonstrate
that the GC worker blocked on a held transaction: the transaction deliberately
drops before the callback resumes. No generic Windows error allowance is used.

## Bounded execution

Build only the library unit test binary, then pass that exact artifact to:

```
python rust/compat/cas_gc_recheck.py --test-binary <library-test-binary>
```

The runner discovers the exact named ignored test and runs only it. Channel waits
are bounded at 10 seconds; the transaction and callback are released before
assertions and thread join. Since `join` itself has no timeout, the enclosing
owned-child watchdog defaults to 45 seconds, kills only its exact spawned test
process on deadline and waits/reaps it. Diagnostic fixture paths are emitted and
retained on failure. Success joins the worker and removes only its owned fixture.
The ignored flag excludes it from ordinary default runs. The native aggregate
uses `--include-ignored`, so it explicitly skips this exact test name and then
requires a separate watchdog step. The dedicated step discovers the freshly built
library artifact from Cargo JSON; it is not an optional or silently skipped gate.

There are no server processes, identities, signals, provider requests, Java,
archives, or ambient stores. Native Windows/macOS results must be recorded
separately; source coverage and local Linux execution are not native acceptance.

## Local evidence

On 2026-10-26, the Linux named test passed through the watchdog and its owned
fixture was verified removed after success. A synthetic sleeping test child
exercised the deadline kill/reap branch and retained diagnostic output. All-target
Clippy and formatting/diff checks passed. Windows code was source-reviewed only;
its actual result remains a native gate, with no error allowance added.

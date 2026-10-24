# Core reliability repair roadmap

This is the bounded maintenance plan authorized on 2026-10-24, based on a
read-only review of `97b65186d7882c415220969d8dcd264b6c3353bd`. It is not a new
feature roadmap or a Rust migration design.

## Scope and completion evidence

1. **Protect client world replacement.** Reproduce the missing running-world
   check with an isolated temporary archive and a held `session.lock`; make
   `worlds.import` and `worlds.restore` reject replacement with `WORLD_LOCKED`
   without changing the original world. Add regression coverage for preserved
   data, cleaned staging, and successful replacement after the lock is no
   longer held. Record the failing-before/passing-after evidence in the PR.
   Use the existing world mutation guard; do not claim it prevents an external
   game from starting after the probe or provides a multi-directory transaction.
2. **Execute Core tests in its own CI.** Add four native standard runners:
   Linux x86_64, macOS arm64/x86_64, and Windows x86_64. Each verifies the pinned
   official Zig 0.16.0 archive, runs `zig build test`, builds ReleaseSafe, and
   checks executable version metadata. Jobs are public-repository-only, have
   read-only repository permission, and do not publish artifacts, use secrets,
   create releases, or run real accounts/games/servers. Platform skips inside
   tests must remain visible; a green job is not proof of skipped behavior.
   In particular, the POSIX-poll RCON and timed-UDP Query transport fixtures
   are skipped on Windows (Zig 0.16 lacks the required transport primitives);
   server operations are Linux-only, while platform-independent parser and
   client-world tests still run.
3. **State interruption guarantees accurately.** Correct the GUI contract and
   installation summaries: copy fallback is not atomic replacement, Windows
   has a separate replacement path, and killing the RPC child does not roll
   back an operation. Reverify incomplete installs; inspect mutable world state
   before retrying. Keep credential-rotation recovery requirements intact.
4. **Verify the integrated revision.** Check the final diff and documentation
   links, run locally available tests/builds, then require the PR's exact-head
   native CI before merging. Verify the merged main commit and its CI separately.
   Record actual results and any blocked checks in the PR; do not substitute
   source test counts or frontend Rust tests for an executed Core test suite.

Native Windows execution exposed pre-existing client I/O failures in account
metadata reads and legacy asset projection copying, plus Unix-only Java/path
test fixtures. The bounded CI follow-through includes correcting those failing
paths and fixtures rather than disabling cross-platform client coverage; the
PR records the failing runs and the eventual verification outcome.

The implementation PR and its CI results are the execution record. This plan
alone does not mark any test, platform, or real-use acceptance as passed.

## Boundaries and deferred work

- The frontend currently pins Core
  `97b65186d7882c415220969d8dcd264b6c3353bd`. This Core-only change does not
  update that pin: adopting the fixed Core and revalidating the frontend is a
  separate downstream step. A merge here is not a distributed application
  release or evidence that users already run the fix.
- Real account/keychain, game, SSH/server, native GUI, and installation acceptance
  remains pending; this repair does not resume the paused frontend acceptance
  project or use another person's computer.
- No broad refactoring, new features, TLS redesign, or atomic-copy rewrite is
  included. Potential performance and other design improvements remain outside
  this repair unless a directly blocking failure is demonstrated.
- A **Rust rewrite** is planned for a later discussion. Additional requirements
  are not yet defined; no migration architecture, parity commitment, new crate,
  or rewrite implementation is authorized by this maintenance roadmap.

## Local verification

Use Zig 0.16.0 from the official distribution:

```sh
zig build test --summary all
zig build -Doptimize=ReleaseSafe --summary all
./zig-out/bin/jmcl-core version --json
```

On Windows, the last executable is `zig-out/bin/jmcl-core.exe`. These checks
are native unit/build/smoke checks, not a real Minecraft or installer acceptance.

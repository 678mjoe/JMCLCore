# Rust rewrite: compatibility before replacement

Status: implementation in progress, **not a drop-in replacement yet**.
Reference implementation: Zig Core commit
[`5a4310debbcee26b3d2391bab2339f79cd6ec52e`](https://github.com/678mjoe/JMCLCore/commit/5a4310debbcee26b3d2391bab2339f79cd6ec52e).
This pins behavior after the world-overwrite and native Windows I/O repairs.

## Binding requirements

- Preserve protocol v1, CLI behavior, and existing disk formats. Existing
  instances, shared objects, public account records, managed runtimes, content
  registries, and backups must be usable without a format-migration step.
- **Hard-link-based file management is the primary project philosophy**, not an
  optional performance optimization. On a supporting filesystem, instances
  materialize verified immutable objects as hard links. Copy fallback remains
  an explicit filesystem limitation, not the new normal.
- Mutable saves, logs, options, and configuration remain private. Sharing an
  immutable managed mod/archive does not authorize sharing its writable state.
- Preserve the boundary between Core execution and caller-owned credentials.
- Produce technical articles alongside implementation, with sources for
  historical facts and explicit labels for interpretation.

The owner authorized autonomous implementation and asked to be consulted when
an uncertain design decision might change a binding constraint. This is not
permission to silently change formats or protocol semantics for Rust convenience.

## Roadmap and gates

1. **Freeze the oracle and inventory.** Capture all 106 dispatched RPC method
   names, CLI entry points, disk schemas, and error/event contracts. Inventory
   is a scope list, not a passing-test count. See [compatibility](compatibility.md)
   and [method inventory](rpc-methods.json).
2. **Build the compatibility harness and transport shell.** Compare both
   binaries on framing, malformed input, protocol/id validation, ping/version,
   stdout/stderr separation, CLI version/help/usage and exit status. Unsupported
   Rust operations remain explicitly incomplete, never counted as parity.
3. **Implement the hard-link storage foundation.** Preserve digest/path/lock
   naming, no-follow validation, exact size/hash checks, read-only publication,
   hard-link materialization, permitted copy fallback, and GC dry-run/grace/link
   count semantics. Verify inode or Windows file identity, not just equal bytes.
4. **Port local state.** Public accounts, instances, managed-runtime receipts,
   content registries, worlds/backups and properties. Exercise Zig-created data
   with Rust and Rust-written data with Zig in disposable fixtures. Verify
   rejection behavior and interruption recovery, not only successful reads.
5. **Port metadata, installation and runtime selection.** Vanilla and loader
   planning, verified downloads, native extraction, managed Java, content and
   modpack flows. Use deterministic fake transports and processor fixtures;
   preserve progress/error contracts and resource bounds.
6. **Port authentication and process behavior.** Fake OAuth/token-rotation
   failures, secret redaction and clearing, client launch event framing, Linux
   detached server lifecycle, FIFO/logs, RCON/Query and stopped-state guards.
   Real credentials, game/server operation and native acceptance stay separate.
7. **Prove the replacement.** Run the full compatibility corpus on Linux,
   macOS ARM/Intel and Windows; account for platform-specific skips. Verify
   frontend contract tests and existing disk takeover. Resolve every uncovered
   method/CLI/storage family before changing the default binary or frontend pin.

## First new feature after the compatibility baseline

The owner requested complete cancellation coverage for long-running Core work.
Track it as the first additive post-baseline feature, separately from parity:
[cooperative task cancellation design](task-cancellation.md) and
[per-method policy inventory](cancellation-inventory.json).
The design is not implemented. Legacy protocol v1 remains synchronous and has
no caller-facing cancellation; a new opt-in task/control layer must preserve
legacy contracts, hard-link ownership, safe cleanup and honest partial outcomes.
This requirement does not mark replacement gates or managed-runtime acquisition
as complete.

Each slice records exact commits, executed tests and remaining gaps in its PR.
A green skeleton or a successful compile does not close later gates. Zig remains
available as the reference and current implementation until replacement gates
are met. Formal releases and real-account/server operations are not implied.

## Decision rule

Internal Rust module layout, ordinary implementation details and test utilities
may evolve autonomously. A change to hard-link-first behavior, store identity,
lock interoperability, credential ownership, serialized format, protocol/CLI
observable behavior, or required caller coordination needs an explicit decision.
A known defect is documented and tested rather than reproduced blindly; any
observable compatibility exception is recorded and resolved before replacement.

Minecraft may be launched for scoped acceptance in isolated test instances when
the required runtime, game files and platform prerequisites are available.
There is no project-wide prohibition on launching Minecraft. Follow applicable
environment access and account authorization requirements; this permission does
not implicitly authorize additional network access or downloads, changes to daily
instances, releases, or promotion to `main`. Record actual launch results
separately from synthetic tests and historical evidence; fixture-specific
no-process guards remain valid. A successful launch does not itself establish
completion of Java runtime acquisition implementation.

## Writing alongside the rewrite

[Technical article index](../blog/README.md). The first article explains the
hard-link model from implementation and commit evidence, together with the
owner's stated motivation: modpacks create many coexisting instances that share
Minecraft versions/loaders/mods, so preserve version isolation without storing
repeated copies of identical files. No specific rejected alternatives have been
identified; source code alone cannot establish that history.

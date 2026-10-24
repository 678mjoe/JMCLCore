# Compatibility inventory and test policy

Oracle: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`. All paths below refer to that
revision. This document specifies work to prove; it does not claim Rust parity.

## RPC surface

The [106-method inventory](rpc-methods.json) is extracted from
[`src/protocol/protocol.zig`](../../src/protocol/protocol.zig). Each domain handler
owns exact parameter validation, defaults, errors and output fields; the older
MVP scope document is not an authoritative list of current capabilities.

Families include versions/install/store, mods/resourcepacks/shaderpacks,
world-scoped datapacks, modpacks, instances, servers and server content/worlds,
Java, launch, Microsoft/Minecraft authentication and public accounts.

Tests must compare:

- UTF-8 JSONL framing, the 64 KiB request limit, CRLF/empty lines, oversized-line
  recovery and EOF behavior against [`main.zig`](../../src/main.zig).
- Required fields/types, empty IDs, unknown fields, validation precedence,
  protocol mismatch, absent/null/object params and unknown method behavior.
- Exact field presence versus null, number/string types and error codes/messages.
  JSON object key order is not semantic; array order and event order are.
- Started/file/retry/progress/processor/runtime/stdout/stderr events, monotonic
  counters, terminal-result/error count and stdout purity.
- A synchronous request per session, separate sessions for concurrency and no
  invented protocol-level cancellation. Same-target mutations still need caller
  coordination; native guards do not eliminate every external-process race.
- Secret-bearing responses and redacted diagnostics separately. Fixtures use
  synthetic secrets, never real tokens; failed token rotation must retain the
  established recovery contract.

Normalization must be narrow and declared per case: fixture-root prefixes,
clock values, process identities and random temporary names may vary. Do not
blanket-remove error strings, paths, counters, source/provider identity or fields
that would hide a compatibility failure. Nondeterministic fields still need
shape/range/relationship assertions.

## CLI surface

Top-level public commands in [`main.zig`](../../src/main.zig): `version`,
`install`, `store`, `java`, `instance`, `server`, `launch`, `rpc`, `help`, `--help`,
and the no-argument help path. Domain parsers in [`src/cli`](../../src/cli) define
subcommands/options, option conflict handling, positional parsing and defaults.
CLI compatibility covers stdout, stderr and exit status, not just acceptance of
similar arguments. Snapshot the current full help output and enumerate parser
branches before declaring coverage. Authentication is an RPC surface; do not
invent a new public auth CLI as a substitute.

Private `__server_bootstrap` / `__server_supervisor` entry points are part of
process orchestration. They need lifecycle tests even though they are not public
help commands. Public CLI parity alone does not prove server restart takeover.

## On-disk inventory

| State | Current contract/source | Required takeover checks |
|---|---|---|
| Shared immutable store | `.jmclcore/objects/sha1/<prefix>/<digest>` and `locks/sha1/<digest>.lock`; [artifact store](../../src/install/artifact_store.zig), [GC](../../src/install/artifact_store.zig) | Zig/Rust object sharing and digest locks, size/hash/no-follow validation, hard-link identity, read-only publication, copied-destination GC semantics |
| Instances | `instances/<id>/instance.json`, schema 1; `.minecraft/.jmcl/install.json`, schema 1; [instance store](../../src/instance_store.zig) | IDs, loader/source defaults, legacy/invalid manifests, exact installation matching, delete scope |
| Public accounts | `accounts/<id>.json`, schema 1; [account store](../../src/auth/account_store.zig) | Public fields, missing/invalid/unknown fields and secret rejection; no credential persistence |
| Managed Java | `<store>/runtimes/<name>/.jmcl-runtime.json`, schema 1; [runtime store](../../src/java/java_runtime_store.zig) | Provider/platform/executable fields and actual probe semantics; native Windows paths |
| Content registry | `<root>/.jmcl/content.json`, schema 1; [registry](../../src/content/content_registry.zig) | Provider IDs, exact filenames, disabled state, unmanaged files; datapack root is its world, not instance |
| Worlds/backups | `saves/<world>`, `backups/*.zip`, `session.lock`, `.jmcl` metadata; [worlds](../../src/worlds/worlds.zig) | Live versus stale locks, overwrite guards, vanilla archive exclusions, naming, traversal/symlink rejection, partial-operation recovery |
| Servers | `servers/<id>/server.json`, schema 1; `.jmcl/install.json`, current schema 3; [server store](../../src/server/server_store.zig) | Existing supported installation-marker versions and launch descriptors, EULA/source/loader fields |
| Live server state | `.jmcl/server.pid`, `.jmcl/console.fifo`, `logs/console.log` rotations; [lifecycle](../../src/server/lifecycle.zig) | PID plus process identity, stale state, detached ownership, log cursors and stop escalation; Linux-only boundary |
| Server properties | Java properties file; [properties](../../src/server/properties.zig) | Preserve existing parser/serialization contract, unknown properties, validation and stopped-state mutations |

Do not silently migrate SHA-1 object identities to another digest or substitute a
new database. SHA-1 here matches upstream artifact identity; it is not a digital
signature or an independent claim of trust in metadata.

## Harness safety and completion

Use disposable roots and independent copies of mutable fixtures for each binary.
Never have the oracle and candidate mutate a real user's world. Hard-link topology
must be constructed intentionally; ordinary recursive copying loses topology and
cannot prove takeover. Read-only object bytes must not be edited through any
instance link. Hash failures, truncated files, forbidden paths, permission errors,
concurrent writers and interrupted operations deserve negative tests.

Record method/CLI/storage coverage separately. A method returning the expected
validation error is not implemented-success coverage. A source list or test
count is not a coverage percentage. Tests requiring real services, credentials
or OS facilities remain explicitly pending or platform-skipped with reasons.

## Open oracle discrepancy decisions

The foundation harness reports known oversized-request differences at the pinned
oracle:

- An oversized line followed by a valid ping in the same buffered input can
  produce two `INVALID_REQUEST` events from Zig; the Rust candidate instead
  emits one invalid-request event and then the ping result. The Zig path clears
  the whole input buffer after discarding the oversized line.
- An oversized unterminated request at EOF exits the Zig process with status 1,
  an `EndOfStream` diagnostic on stderr and no RPC event. The Rust foundation
  already preserves this separate EOF behavior; it is not a known mismatch.

Whether to repair the buffered-next-request defect in the oracle before aligning
Rust is pending the owner's compatibility decision. Keep exact harness fixtures and record the resolution;
do not silently normalize them away, modify the oracle, or count these cases as
parity while unresolved. Independent storage and ordinary protocol work can
continue.

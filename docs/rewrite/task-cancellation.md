# First post-baseline feature: cooperative task cancellation

Status: **design proposal, not implemented or advertised by Core**.
Baseline: Rust branch commit `03ad49d787a6493fd2393b1f2f0d04dd91fbab74`;
legacy oracle `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
The owner requested cancellation coverage for Core operations on 2026-10-25,
either during the Rust rewrite or as its first new feature. This proposal chooses
the latter delivery boundary so compatibility work is not silently redefined.
It does not say that replacement readiness has already been achieved.

## Scope and current evidence

- Protocol v1 deliberately accepts one request at a time. Rust
  [`protocol::run`](../../rust/src/protocol.rs) invokes the handler before reading
  more input; Zig [`runRpc`](../../src/main.zig) does likewise. See the
  [existing GUI process contract](../gui-handoff.md#1-process-model).
  Adding a queued `cancel` method alone cannot interrupt a running operation.
- Internal `AUTH_CANCELED`, request timeouts and fallible download/event callbacks
  are not a public user cancellation API. The blocking HTTP adapters, retry waits,
  filesystem lock waits and process waits need explicit interruptible boundaries.
- Inventory all [106 existing methods](cancellation-inventory.json). A complete
  design means a declared policy for each method, not 106 redundant cancel RPCs.
  Short reads/metadata writes can finish before cancellation; server shutdown is
  distinct from cancelling the command that requested shutdown.
- This document does not reopen incomplete managed-runtime acquisition work.
  Its cancellation policy is a dependency requirement, not an implementation or
  validation claim for that path.

## Compatibility boundary and proposed control plane

Keep `jmcl-core rpc`, protocol v1 method results/events, existing CLI behavior and
on-disk data formats unchanged. Add an explicitly opted-in, versioned task mode
as the first post-baseline feature. Recommended wire boundary: a new protocol
version/mode with capability discovery, **not concurrent handling silently enabled
for existing v1 callers**. Exact mode flag and wire schema require review before
implementation; the following names are illustrative, not reserved API promises.

The new mode needs start/admit, status and cancel controls (for example
`task.start`, `task.get`, `task.cancel`), plus task progress and terminal delivery.
A dedicated input/control reader must continue accepting cancellation while a
bounded worker pool operates. One serialized output writer prevents JSONL line
interleaving. Control acknowledgement must not wait behind saturated download
workers, lock waiters or an unlimited progress queue; progress may be coalesced,
but terminal results cannot be silently dropped. Slow or disconnected output is
itself a tested shutdown path, not permission to run unbounded detached tasks.

- Request IDs correlate individual control replies. Core-generated opaque task
  IDs identify jobs and have a session-generation namespace. They are not PIDs,
  paths, account IDs, credentials, or caller-reused request IDs.
- Cancellation targets only tasks owned by the same authorized session. No
  cross-process PID-based cancellation or unauthenticated control socket.
- Existing filesystem locks remain authoritative; scheduler admission additionally
  serializes conflicting mutation targets. Download concurrency does not authorize
  concurrent mutation of the same instance, registry or world.
- Nested actions share the parent cancellation context. A modpack workflow must
  not accidentally leave its subsequent game preparation running after cancel.
- MVP status is bounded in-memory task history, not a general persistent daemon.
  Closing a GUI panel is not closing Core's session. Frontend keeps its native
  session alive for background work. Actual IPC disconnect requests cooperative
  cancellation, cleanup and worker joining; it is not an immediate process kill.
  Already handed-off games/servers are not killed as disconnect cleanup; their
  explicit stop contracts remain separate. Process crash remains a recovery case,
  not a clean cancellation receipt.
- Persist only narrowly needed operation-recovery state, if necessary, as a
  versioned additive sidecar. Never alter existing instance/CAS formats casually.
  Task status/history must not retain raw parameters, token results, HTTP headers,
  command output or user secrets. Auth secrets retain their existing caller-owned,
  single-delivery/clearing obligations and are not replayed by task status.

## Outcomes and race semantics

Proposed lifecycle: `queued → running → cancelling → cancelled`, or
`queued/running/cancelling → succeeded/failed`. Terminal transitions are exclusive.
The original task receives exactly one terminal outcome; a cancel request receives
its own acknowledgement. Exactly one terminal state is a Core invariant, not a
guarantee that a disconnected caller received that message. Lost delivery is
reported/recovered explicitly, never treated as acknowledged. Accepting cancellation means “request registered”, **not**
“all work stopped” or “rollback complete”. Repeated cancellation is idempotent while the task record is retained. After
bounded-history eviction, return an explicit expired/not-found result; never
reuse its task ID for another operation.

At each operation's documented commit boundary, arbitrate completion versus
cancellation under the same state synchronization. Before the boundary, stop new
work and clean only owned staging. During a short critical publication section,
finish establishing a valid state before honouring cancellation. After an
irreversible action, return `too_late`/already-terminal status where appropriate,
or an explicit interrupted-partial outcome for multi-step operations. Never emit
“cancelled with no changes” when a world, server or remote service was changed.

Terminal status includes a sanitized phase, committed-effects summary, cleanup
status and whether inspection/recovery is required. Cleanup failure must be
observable, not swallowed into success. No path or secret must be disclosed merely
because it appears in an operating-system error. A fixed safe error vocabulary
is preferable to raw exception text.

Cancellation is cooperative, not arbitrary thread termination. Every phase must
have a measured response budget and bounded work chunk (stream reads, digesting,
archive entries, directory scans, retry backoff, lock waits and child wait loops).
A cancellation flag checked only when progress is emitted is insufficient: stalled
network responses and lock acquisition must also wake or have a declared bound.
Do not promise a universal instantaneous cancel. First measure per-platform
bounds with deterministic barriers, then publish the supported latency contract.

## Operation policies

The machine-readable inventory assigns every legacy method to one policy below.
These are proposed requirements, not current capabilities. Method families have
additional commit boundaries; `short-call` does not assert that every underlying
write is a transaction or can be undone.

| Family | Proposed cancellation and completion boundary |
| --- | --- |
| Metadata/plan/network queries; Java detection | Abort waits and stop enumeration/probes. Pure returned data may be discarded; kill only a task-owned probe with verified process identity. Existing cache publications may remain valid. |
| Downloads, install/prepare, content install/update/adopt | Stop scheduling, interrupt bounded I/O and retry waits, join workers. Remove only operation-owned unpublished files; retain verified CAS publications. Preserve coherent registry/receipt state. Do not mark an incomplete install as installed. |
| Modpack import plus follow-up preparation | One frontend-visible workflow with stage-specific task ownership. Cancelled initial import may remove only the exact newly created instance generation, never a pre-existing/same-ID replacement. Already committed stages must be reported; resuming setup is not blindly importing again. |
| World duplicate/export/backup/import/restore | Stop copy/ZIP work before publication. Clean staging, preserve prior destination; do not interrupt the short old→trash/new→target publication sequence. Multi-world restore has per-component commits, not global rollback. Retain a recovery journal or declare exact partial effects. Preserve running-world guards. |
| Deletion, removal and GC | Stop between safe units. Already deleted entries cannot be restored by a cancel request. Report counts/partial effects; keep GC grace, link-count and object-lock rules unchanged. Never resurrect deleted files or pretend deletion was atomic. |
| Auth begin/poll/exchange/refresh | Stop pending polling/network work where possible. Refresh/exchange may have changed remote token state before cancellation is observed: once rotated credentials are available, deliver the required recovery result rather than discard it to manufacture `cancelled`. Lost IPC retains existing account recovery rules. Cancellation is not token revocation. |
| Client launch and loader processors | Before spawn, stop preparations. After spawn, distinguish task-owned installer/probe children from a launched user game. Provide explicit process-stop semantics for running games, not implicit kill on panel close. Task-owned subprocess shutdown needs bounded grace/escalation, process identity and descendant ownership on each OS. Do not signal arbitrary/reused PIDs. |
| Server start/stop/restart | Cancelling a wait does not undo a spawned server or restart a stopped one. Report current process state; use existing explicit lifecycle operations. Once graceful stop was delivered, do not reverse it. Restart must account for the stop/start boundary. |
| Remote command/RCON/query and logs | Stop local socket reads/log-following; an already sent command cannot be unsent, and absence of reply does not prove no side effect. Return explicit unknown-effect/recovery state where necessary. Closing an SSH frontend channel is not evidence a remote operation stopped. |
| Short getters, ping/version, metadata/registry writes | If complete before cancellation, return completed. Check admission before starting; avoid artificial rollback of a committed short write. Getters involving scans still use bounded/cooperative read policy where applicable. |

## Hard-link and mutable-state invariants

The hard-link storage philosophy is binding, not an optimization to simplify away:

1. Never truncate, rewrite or chmod a shared CAS inode as cancellation cleanup.
   Cancelled workers may remove their owned temporary name or instance link, but
   cannot delete a shared object as “rollback”. Verified published CAS objects
   remain reusable, and normal locked GC decides later eligibility.
2. Keep per-digest locking through verification/publication/materialization;
   release guards on cancellation without deleting a lock file underneath waiters.
   A cancelled waiter cannot release another task's lock or remove its staging.
3. Track target generation/ownership, not merely textual instance ID. Current
   [modpack cleanup](../../rust/src/modpack/service.rs) deletes the newly created
   instance on failure; an asynchronous scheduler must keep that ownership true
   throughout cleanup and exclude conflicting mutations.
4. Private saves/configuration/overrides remain private. A restored world is never
   partially written through a shared inode. Preserve no-follow path checks,
   permitted world aliases, and publication locks while adding checkpoints.
5. Existing [world publication](../../rust/src/worlds/archive.rs) has a short
   rename/rollback sequence and a distinct multi-component server-world sequence.
   Do not advertise whole-workflow atomicity or undo committed components merely
   to simplify the task state machine. Expose failed cleanup and recoverable trash.

## Delivery order and acceptance gates

C0. Freeze this per-method policy inventory and review wire/ownership semantics.
No cancellation capability is exposed at this stage.

C1. Build the opt-in control plane, task IDs, bounded scheduler/event writer,
terminal arbitration and cancellation token with synthetic non-I/O jobs. Keep
v1 oracle corpus unchanged. Capability reporting lists supported operations,
not just a global “supports cancellation” boolean.

C2. Make waits and adapters cancellation-aware: networking, retry/backoff, hashing,
filesystem traversal, lock waiting, task-owned subprocesses and event backpressure.
Tests must prove prompt control acknowledgement while a worker is stalled.

C3. Wire install/content/modpack/preparation and world archive families. Verify
cleanup, ownership, commit boundaries and safe retry. Managed runtime acquisition
remains a separately incomplete dependency; do not claim all-cancel completeness
until its implemented path eventually meets the same contract.

C4. Cover auth and lifecycle/external-effect semantics, then short-call/deletion
policies. Fill the entire inventory; no silent “cancel requested” stubs. Document
CLI interrupt and remote-session limits explicitly without changing legacy CLI
behaviour by accident.

C5. Expose frontend capability-gated Cancel controls and separate “run in
background” from cancellation. Show cancelling/cleanup, completion-race, partial
and recovery states. Add restart/disconnect recovery presentation. Only advertise
coverage after exact-head Linux, macOS ARM/Intel and Windows gates pass.

Required tests include cancel before start, every blocking phase and immediately
before/after commit; repeated cancel and simultaneous completion; queued task ID
reuse/restart isolation; full output queue/broken writer; wrong-owner cancellation;
partial cleanup failure; lock owner/waiter cancellation during competing GC; two
instances sharing a digest (unchanged sibling bytes, file identity and read-only
attributes); no temporary/lock leaks; world replacement and multi-world partial
commit; token-rotation response races without real secrets; owned process trees
versus unrelated/reused PID; and re-open/reverify/retry after crash. Use disposable
fixtures only. UI and real-account/game acceptance remain separate evidence.

The auth requirements above preserve existing credential-rotation recovery; they
do not introduce a new consent, revocation, credential storage or account policy.

## Decisions still required before freezing the new wire contract

The user has authorized the feature goal, not a concrete wire schema. Review the
new protocol/mode spelling, task-result/event framing, retained-history limits,
and the explicit operation used to stop a launched game. Recommended defaults
above avoid modifying legacy behavior. These are ordinary design review points;
no user decision is needed merely to begin the bounded design/inventory work.

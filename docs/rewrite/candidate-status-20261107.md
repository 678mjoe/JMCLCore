# Candidate status after scoped Linux acceptance, 2026-11-07

Rust is still an experimental candidate. `replacement_allowed` and
`full_compatibility` remain false. The audit's 106 method names and 105 static
handler associations are not 105 compatible methods. No universal compatibility
percentage is claimed. The default frontend/package workflows still use Zig.

This index reconciles successive slices without altering their historical
claims. Exact tested commits, trees, commands, source hashes and log hashes are
in each linked record. Later evidence-only commits do not imply rerunning older
checks against their whole-repository tree.

## Passed within explicit bounds

- [Nov 6 native Session chains](candidate-acceptance-20261107.md#3-executed-evidence-retained-in-this-tree):
  synthetic instance/content and benign-child launch. Their preserved bytes and
  provenance were checked Nov 7; those original guarded chains were not rerun.
- [First Nov 7 Linux slice](candidate-followup-20261107.md): offline locked
  all-targets compile, Clippy, exact URL construction pairs, local persistence,
  content recovery, synthetic auth, benign native process launch and pure GUI
  launch construction. Not a real account, game or Tauri UI.
- [Metadata audit](../../rust/compat/metadata-diagnostic-audit-20261107.md): 25
  existing tests, 5,015 source-equal comparisons, plus one explicitly accepted
  URL difference. Earlier concrete parser gaps have bounded fixes/evidence;
  the old blanket debt statement is not a current known failure.
- [Client worlds](worlds-candidate-20261107.md): owned NBT/region/world-ZIP
  operations, 7 exact tests and 9 paired CLI scenarios with 48 command results.
  These are not Java runtime archives or Minecraft-generated worlds.
- [Server lifecycle](candidate-server-followup-20261107.md) and
  [handshake/loopback](candidate-server-extended-20261107.md): real Rust RPC
  lifecycle and local transports around owned shell fixtures, verified selection,
  failure cleanup, RCON and basic Query. Installed JVM property/version probes
  actually execute in these later slices; the JVM is not the server process.
- [Modpack install/cache/rollback](modpack-candidate-20261107.md): two exact
  production-service tests with owned mrpack ZIPs, injected bytes, CAS sharing,
  private overrides and retry. No Java, provider or actual Core RPC.

These counts describe different evidence layers and must not be summed into a
completion fraction. COMPAT-URL-001 remains the six named accepted construction
and representation differences. A new byte-preserving transport is **not** a
required replacement blocker, and acceptance does not waive unrelated security,
HTTP/TLS/redirect or parser requirements.

## Important remaining scope

| Area | Current classification | Concrete prerequisite / next decision |
|---|---|---|
| Managed Java acquisition (`java.runtime.install`) | Confirmed unimplemented; automatic acquisition/runtime-archive installation remains outside the allowed execution scope. No attempt in these slices. | Resolve the implementation and permitted validation scope explicitly. Ordinary installed-JVM probes are distinct and have now run. |
| Native Windows/macOS support | Not run in this Nov 7 acceptance. No new native pass inferred from target strings, pure plans or Linux builds. | Use an authorized native executor with pinned toolchain/cache and platform-specific nonzero test selectors. Do not substitute broad CI or start a cloud task. |
| Live accounts, provider metadata/artifacts and service transport | Synthetic transport and pinned parser/construction evidence only. | Identify an authorized real account/provider, endpoint and data scope; separately verify live errors, HTTPS/TLS, redirect and authentication behavior. |
| Real installed-JVM game launch and loader processors | Not run; benign native children and shell fixtures are not Minecraft or loader execution. | Reviewed actual runtime/game/assets/loader inputs and a bounded owned install/launch plan, separate from prohibited automatic runtime acquisition. |
| Actual Tauri frontend with Rust sidecar | Not run; frontend CI and preview packaging use pinned Zig. | A separately scoped Rust-sidecar integration on an authorized GUI/native executor, then the real account/instance/content/launch UI flows. No production switch follows from this report. |
| Remaining family acceptance | Finite reviewed cases, not full suite or universal compatibility. | Prioritize an actual unverified normal flow or a reproducible contract defect. The reviewed synthetic modpack install/cache/rollback slice is now recorded above; it does not cover real provider acquisition, launcher installation or loader processors. Do not assume every archive is a prohibited Java runtime archive. |

No tool action was denied during the documented slices. The missing runtime
acquisition capability, a deliberately excluded operation, an unavailable native
executor and an unrun acceptance case are different states; none should be
misreported as a tool denial or a permanent ban. The synthetic tests do not
remove the listed prerequisites or authorize merging/releasing the candidate.

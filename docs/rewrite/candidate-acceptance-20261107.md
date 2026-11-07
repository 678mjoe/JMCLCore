# Rust candidate and GUI acceptance, 2026-11-07

This is the current evidence map for the preserved candidate below. Read it
before interpreting the historical milestone sections or a green test count.
The Rust candidate is **not approved to replace the Zig Core**.
`full_compatibility` and `replacement_allowed` remain false.

Subsequent Linux compile/Clippy and scoped executable results are recorded
separately in [the follow-up](candidate-followup-20261107.md). The historical
scope and checks below describe the original documentation audit.

A later, separately scoped [server lifecycle execution record](candidate-server-followup-20261107.md)
adds installed-JVM property probes and owned synthetic server execution. It does
not change the no-real-Java scope of the earlier results described here.

The separately reviewed [client-world follow-up](worlds-candidate-20261107.md)
adds owned synthetic NBT/region/world-ZIP workflows and pinned CLI comparisons.
It does not claim Java runtime archive installation or real game generation.

See the [current scoped status matrix](candidate-status-20261107.md) for later
handshake/loopback, four-platform wrapper/Session and release-discovery results,
and the separate Windows adoption identity result. The matrix distinguishes
frozen Core, executed harness/test trees and later results-document commits; it
does not claim one combined tested tree or full Tauri/game acceptance.

## 1. Exact subject and distribution boundary

| Subject | Identity and meaning |
|---|---|
| Preserved candidate | [`rust-native-launch-verified-20261106`](https://github.com/678mjoe/JMCLCore/tree/fed258eb7c1042494219bea789d092df15dbea50), commit `fed258eb7c1042494219bea789d092df15dbea50` |
| Exact candidate tree | `df5f45d780b5f8443f2f5f57f3750e6f2b604896` |
| Earlier local identity | The preservation commit records the same tree as local `0b3c8bd531947f3686203ab168817185d47bf2fd`. That original local commit/worktree was absent from the restored environment. |
| Recovered source | All 749 tracked blobs and file modes were reconstructed from the GitHub tree and verified Git blob objects. Local recovery commit `aa2a6dd60e6aabd96dc5750cca0b4ba21ba75250` has exactly the candidate tree. Its commit history is local recovery history, not the missing original history. |
| Production reference | Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, the pinned behavioral oracle and frontend sidecar source. |
| Rust executable | `jmcl-core-rust`; Cargo package `publish = false`. Wire identity intentionally remains `jmcl-core`, protocol 1, version `0.1.0-dev`. It does not delegate operations to Zig. |
| Frontend source | Main snapshot `9c0fd81ce8109c1e35229560fee31308ca114ef5` (PR #28). Its [quality workflow](https://github.com/678mjoe/JMCL-frontend/blob/9c0fd81ce8109c1e35229560fee31308ca114ef5/.github/workflows/quality.yml) still builds the pinned Zig sidecar. Frontend CI/package success therefore does not validate a Rust sidecar. |

The remote candidate branch/ref, commit-to-tree association, recursive tree and
all restored blob identities were checked read-only on 2026-11-07. The recovery
and checks do not change `main`, PRs, releases, CI configuration or production
source. Preserving this documentation on a separate experimental branch does
not promote the candidate or establish additional runtime evidence.
The local follow-on documentation commits do not pretend to retain the same
whole-repository tree; their production source is unchanged from the candidate.

The frontend's separate desktop-preview work also pins Zig and is outside this
Rust acceptance claim. Unsigned preview packaging is not a Rust release or a
live GUI/game test. Refer to that task's exact run for current package status.

## 2. What exists in the current Rust source

The digest-bound [dispatch audit](../../rust/compat/dispatch-audit.json) passes
against this candidate: **106 original method names, 105 static handler
associations, one explicit unavailable method (`java.runtime.install`)**.
These are routing classifications, not 105 fully compatible methods or a
completion percentage. Zero methods are claimed universally compatible in
[coverage.json](../../rust/compat/coverage.json).

| Contract family | Current source and evidence boundary |
|---|---|
| Protocol and CLI | Serial JSONL protocol, handshake, malformed-request recovery, native argv routes and CLI diagnostics exist. Source-specific approved deviations remain explicit. |
| Accounts/authentication | Public account persistence and authentication handlers exist. Synthetic token/transport cases do not establish real login, token rotation, keychain or credential-injection acceptance. |
| Versions, instances, installation | Version list/get/resolve/plan, default validating instance creation, cached metadata, verified artifact/CAS execution, receipt identity and preparation exist. Vanilla and loader paths are present; live download/provider behavior and processor/runtime prerequisites remain separate gates. |
| Java | Local detect/select, existing managed receipt list/remove and policy ordering exist. Explicit override and installed/local resolution have bounded evidence. Runtime acquisition is missing. |
| Launch | Plan and execute, local/explicit executable selection, process events, redaction, framing and cleanup exist. The native chain below runs a benign owned child, not Java or Minecraft. |
| Local/provider content and modpacks | Local list/adopt/toggle/remove and provider-backed install/version selection and modpack paths exist. The native lifecycle below covers ordinary local bytes; provider acquisition and real archives are not covered by that lane. |
| Worlds and storage | World inspection/mutation/backup/import/export and immutable hard-link-first CAS exist, with historical differential and disk tests. Mutable saves/configuration remain private. Receipt/download completion must not be confused with successful native preparation or a playable game. |
| Server families | Linux server create/install/lifecycle/logs/commands, RCON/Query, content and world-set handlers exist. Server RPC is Linux-only; some read/delete-backup CLI adapters are portable. Synthetic server fixtures are not live server or SSH acceptance. |

The early Rust README statements that network instance validation was unported
and server support consisted only of codecs describe the first milestone.
They must not be used as a current missing-feature list. Conversely, the presence
of current handlers does not close their ordinary parameter, event, disk,
transport or platform acceptance requirements.

## 3. Executed evidence retained in this tree

The following results were already captured on 2026-11-06. This audit verified
their stored source/trace identities; it did **not** rerun their processes.
The three lanes report the same Core executable SHA-256:
`dad858d76f05cc4fad65814b03790a83e02e9244c5fa1d3155f882b89c1fd1f5`.

| Lane | Recorded passing scope | Explicit boundary |
|---|---|---|
| [Native instance](../../rust/compat/frontend_bridge/INSTANCE_CHAIN.md) | One exact test, 1 passed / 0 failed / 0 ignored. Unchanged native Session → actual Rust dispatcher (not a mocked resolver) → cached synthetic install/prepare; events, read-only shared CAS inode identity, receipts, prepare error, retry and close/reopen. | Linux x86_64 LP64. Synthetic `hello` artifact, no archive parsing, Java or game. |
| [Native content](../../rust/compat/frontend_bridge/CONTENT_LIFECYCLE.md) | One exact test, 1/0/0. Real Session/Core list/adopt/manual-version rejection/toggle/remove across two instances; idempotent cases, disabled persistence, peer independence and CAS links. | Mods/resourcepacks contain plain `hello` bytes. No provider request or archive execution. Linux only. |
| [Native launch](../../rust/compat/frontend_bridge/LAUNCH_GUARD_VALIDATION.md) | One exact test, 1/0/0, under the separate reviewed launch-only guard after mandatory self-tests and benign missing/successful spawn gates. Missing override, same-session retry, child exit 7 versus 0, started/base64 stdout/stderr, redaction, empty newline and split-UTF-8 framing, child reaping, native cleanup and reopen. | Benign compiled Rust child with explicit owned override/local policy and synthetic auth. No JVM/game, account, runtime acquisition or live running-operation cancellation. Linux only. |

The final launch trace is
[`native-launch-frontend-shape-20261106`](../../rust/compat/frontend_bridge/traces/native-launch-frontend-shape-20261106).
It uses actual frontend-shaped params (`instances/demo/.minecraft`, explicit
override, local policy, omitted `java_paths`). It records original Session
callback objects and returned Result/SessionError values, **not raw Core wire
bytes**. Normalization replaces only the literal owned fixture root. The audit
verified all ten source fingerprints in its provenance and these file digests:

- `raw.jsonl`: `e87118512ff5a7d7da2fcad6652fb3f788108bf4f0abfed019ed572158a34e3c`
- `normalized.json`: `f117361e6a1222de3f169b706edb11fd937b8813a0d8b4d3a006e4815bec5c34`
- `provenance.json`: `335d1d70a1f47be11c42fcf92b40ea77743587fa7f8f47b0cc6777dc92330597`

Frontend Session/transport/modpack-preview modules were imported unchanged from
`c0107f1803c73398a99f8ddd27ed712c0ed2e1b4` and recorded byte-identical to frontend
snapshot `90b9d5472e620255af740b55315e2cf75c1c9115`. That comparison covers only
those three modules, not every frontend file or a mounted UI.

The original-guard launch failure remains preserved. The later passed run names
its different, independently reviewed guard; it does not rewrite the old failure.
These optional lanes are default-off and are not enabled by the existing CI
workflow. `--all-features` would enable them and is not a harmless substitute.
Imported frontend transport has five recorded Clippy warnings; warning-free
status for these harnesses is not claimed.

Historical milestone/native CI and bounded diagnostics remain useful evidence
for their own pinned sources. Their counts must not be summed into current
coverage or reused as an exact-tree all-platform pass. Similarly, the older
four-platform frontend bridge workflow uses frontend `91ec19d...`, not the
new native-chain pin. A workflow definition is not an executed result.

## 4. Platform support versus acceptance

The table below describes only the three retained November 6 native-chain
lanes, not all later native execution. The [current matrix](candidate-status-20261107.md#later-native-evidence-separate-experimental-trees)
records subsequently passed four-platform wrapper/Session and release-resolver
contracts plus one Windows file-identity test, each on its stated subject.
Those narrower results do not rerun these three lanes on every platform.

| Target | Source/workflow scope | This candidate's new native-chain evidence |
|---|---|---|
| Linux x86_64 | Desktop implementation plus Linux server RPC. Native CI target is Ubuntu 24.04. | Retained synthetic instance, content and benign-child launch runs passed. Full real GUI/game acceptance is still open. |
| macOS ARM64 | Desktop implementation and macOS 15 workflow target. Server RPC rejects as unsupported. | No execution of these three native-chain lanes. Historical CI is not current candidate certification. |
| macOS x86_64 | Desktop implementation and macOS 15 Intel workflow target. Server RPC rejects as unsupported. | Same remaining gate. |
| Windows x86_64 | Desktop implementation and Windows 2025 workflow target. Native filesystem/process semantics need their own runs; server RPC rejects as unsupported. | Same remaining gate. |
| Other targets | Some portable helpers recognize additional targets. This is not a supported distribution or acceptance promise. | No claim. |

Cross-compilation can check types/configuration, not native lock behavior,
file identity, process execution, Tauri integration or UI behavior.

## 5. Actual remaining capabilities and acceptance work

### Accepted URL deviation, not an implementation blocker

[COMPAT-URL-001](../../rust/compat/url-compatibility-decision.md) retains the
existing Rust URL/HTTP backend. Its six named cases are `unicode_path`,
`dot_segments`, `escaped_dot`, `raw_ff_path`, `raw_ff_query` and `surrogate_path`.
The retained Unicode/dot-segment normalization and unsupported opaque non-UTF-8
URL representation are accepted compatibility differences, **not a replacement
blocker requiring a dedicated byte-preserving transport**. The separate pure
raw-URL prototype was not integrated and must not be presented as mandatory
production work or as authorization to change that decision.

This is a compatibility choice, not a Zig defect or wire-parity claim. The
32-case construction fixture contains those six exact differences, eight strict
target controls and eighteen construction-only diagnostics; it does not prove
network transmission. Ordinary parser/error fidelity, unrelated URL inputs,
HTTPS/TLS, host/SNI, credential-origin and redirect restrictions, real provider
behavior and native execution of the corpus remain independent requirements.
Do not extend the decision into a blanket mismatch or security-policy waiver.

### Remaining work

1. **Missing implementation: managed Java acquisition.** RPC
   `java.runtime.install` reaches `NOT_IMPLEMENTED`; CLI install reports the
   same development limitation. Production launch/loader orchestration still
   contains an unavailable downloader callback. Returning `JAVA_DOWNLOAD_FAILED`
   there is not a successful runtime installer or downloader parity. Existing
   runtime receipts/local selection do not close this. Java automatic acquisition
   and archive-install operations were not attempted by this audit.
2. **Real native frontend integration.** The later four-platform wrapper/Session
   and release-mode adjacent discovery slices have passed on their explicit
   subjects. The frontend baseline's actual Tauri crate check and Zig-backed
   debug package checks also passed, as linked in the current matrix. Actual
   Rust-sidecar packages, full Tauri IPC/event delivery, mounted
   task/content/launch UI, native credential injection and keychain behavior
   remain unaccepted. A standalone Session/resolver probe validates its own
   layer only; it is not a full Tauri application or installed bundle.
3. **Real service and game flows.** Authentication and token rotation, provider
   HTTP/TLS/redirect/error behavior, actual Java/game and loader processing,
   archive compatibility, and live server/SSH behavior require their own scoped
   prerequisites and acceptance. No live accounts or game/server deployments
   were used here.
4. **Remaining current-source native acceptance.** Later finite cross-platform
   results are in the current matrix; "no Windows/macOS run" is no longer an
   accurate overall summary. They do not cover all business flows, oracle
   prerequisites, ignored cases, links/locks/copy fallback or disk takeover.
   Windows original-file adoption has its own tested tree and does not establish
   cross-volume behavior. The broad suite was not rerun by this documentation
   audit, and no combined test tree is implied by the separate results.
5. **Separate post-baseline feature: cancellation.** Protocol v1 intentionally
   remains serial with no caller-facing cancel operation. Cooperative task
   cancellation is a separate post-baseline feature described in the
   [existing design](task-cancellation.md), not an existing parity promise.
   Close/reopen after completed operations does not validate interrupting or
   rolling back a running install, game or credential rotation.

Before release or replacement, resolve the missing capabilities, then obtain
exact-tree native and GUI acceptance. Do not substitute extra malformed-input
cases for these ordinary-flow gates.

## 6. Two corrected normal-path GUI instructions

The [GUI contract](../gui-handoff.md) had two source-proven documentation errors:

- Offline UUID was described as raw MD5. Both Zig and Rust set UUID v3 and RFC
  variant bits. An independent GUI following the old text could assign the same
  player name a different identity from CLI `--offline-name`. The contract now
  states the full algorithm and fixed `Notch`/`Steve` vectors.
- `java_override` was described as mutually exclusive with `java_policy`.
  Both launch.plan and launch.execute actually reject an override combined with
  non-empty `java_paths`. A valid policy can accompany the override; the override
  takes precedence. The preserved native launch trace already uses this form.

Production behavior was already correct and was left unchanged. This audit did
not establish another safely scoped missing Rust production feature; it does
not relabel documentation corrections as newly implemented Core capabilities.

## 7. Checks performed in this audit

- Exact candidate tree, all blob/mode identities and final trace provenance.
- `validate_coverage.py --static-only`: 106 names / 105 associations / one
  unavailable method, zero Core requests.
- `validate_coverage.py --oracle-source <pinned source>`: original inventory
  matches, zero universally compatible methods claimed. Only source and Git
  revision inspection, no Zig executable.
- All 22 existing dispatch-audit guard tests in normal and optimized Python.
- Five new `test_gui_handoff.py` documentation/source/preserved-trace checks in
  normal and optimized Python, including a guard against reclassifying the
  accepted COMPAT-URL-001 cases as a new implementation blocker.
- Direct standalone Rust compilation of the existing pure `offline_uuid.rs`
  helper with `rustc --test`: its one algorithm test passed, 0 failed/ignored.
- Repository whitespace checks and relative documentation links.

No Core session, Java/JVM/game, acquisition/archive installer, network provider,
guard process, native chain or full application was executed by these checks.
The checks themselves make no remote writes or production promotion. Separate
experimental-branch preservation does not change the evidence boundary. The
preserved runtime evidence above and today's static/pure checks remain
separately labeled.

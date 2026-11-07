# Candidate status after scoped native acceptance, 2026-11-07

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

- [Host-accurate probe fixture](probe-fixture-arch-20261107.md): test-only
  architecture/bit-width reporting and assertions, with Linux x86_64 native
  verification. This is not new ARM64/macOS/Windows execution or a production
  managed-runtime selection change.

## Later native evidence: separate experimental trees

The existing frontend baseline is also verified separately:
[quality run 42201686343](https://github.com/678mjoe/JMCL-frontend/actions/runs/42201686343)
completed successfully at frontend `9c0fd81ce8109c1e35229560fee31308ca114ef5`.
All four native jobs passed the actual `src-tauri` crate check, Rust unit/local
RPC tests, and unsigned debug package generation/inspection. That workflow pins
Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`; it is **not** a Rust-sidecar
Tauri command/UI or Rust package runtime result. A claim that the real Tauri
crate has never been checked would be incorrect.

The following completed runs were rechecked read-only, including every job's
head SHA and successful conclusion. They extend the earlier Linux results;
they do not rerun the November 6 instance/content/benign-launch chains on every
platform, and their harness/test trees are **not one integrated tested tree**.

Both four-platform lanes use frozen Core
`bdd6cb3e55d69cb9e0e02037d251635e289207a8`, tree
`0e8e901bc587cf3c6d7dc1ba78821c4701785e20`, and frontend
`9c0fd81ce8109c1e35229560fee31308ca114ef5`, tree
`48ea91981bdb7b375bca8e9bc47738b6c3dde4d1`.
Their native platforms are Ubuntu x86_64, macOS ARM64, macOS x86_64 and Windows
x86_64. The results documents below are later evidence commits, not the executed
harness commits.

- **Typed wrappers and native Session/Core contracts:**
  [run 42388086740](https://github.com/678mjoe/JMCLCore/actions/runs/42388086740)
  passed all four jobs. Each ran the two existing account/capability wrapper
  scripts and six exact Session selectors: handshake, ping/error survival,
  empty runtime listing, instance-status matching, serialized callers and
  synthetic account metadata. Executed harness
  `d697c8fd12766979aed4c3190a3b19fc094b4f7d`, tree
  `b7d8af853e43ca401eb8252b078f9ec58b913561`.
  [Results at d52175d](https://github.com/678mjoe/JMCLCore/blob/d52175d39004c837b546b330059259a432372994/docs/rewrite/native-frontend-contract-results-20261107.md).
  This is typed-wrapper and byte-identical imported frontend Session/transport
  acceptance, not the full Tauri crate, IPC, mounted UI, real account or game.
- **Release-mode adjacent Core discovery:**
  [run 42390179188](https://github.com/678mjoe/JMCLCore/actions/runs/42390179188)
  passed all four jobs, each with present-adjacent and absent-sidecar scenarios.
  Without explicit binary overrides, the real release-built resolver/probe
  finds the adjacent Core and completes handshake/ping/close, or reports the
  exact missing-Core error. Executed harness
  `d596f19a408a8daf4e003998f8116882c7fdc399`, tree
  `9cf5b2283d6a4c439e715737d84d4cb36baa19b1`.
  [Results at 2904fad](https://github.com/678mjoe/JMCLCore/blob/2904fad3a70434999d4a885b682add72f1b41926/docs/release-discovery-ci-20261107.md).
  The probe is release-profile; Core is a native **debug** build. macOS bundle
  directories are simulated. No actual Tauri package, installer, signing or
  production release build is accepted by this result.
- **Windows original-file adoption identity:**
  [run 42391458369](https://github.com/678mjoe/JMCLCore/actions/runs/42391458369)
  passed its sole native Windows job: exact test
  `adoption_reuses_real_file_identity_and_manual_registry` in `content_adopt`,
  1/0/0, with two oracle tests filtered out. Its tested commit is
  `e0ec798e863c877c3fde77f1f86c1e06c5583d3a`, complete tree
  `5f4599323c9a17bef83737f2890326e0d38f2ee6`, **not** the frozen Core tree above.
  It adds test assertions and a separate harness; production `rust/src` remains
  `99bfa7577af7597ab8069aecd278bb4e9a352296`.
  In one owned same-volume TempDir, the original file's handle volume/file index
  equals source and CAS after both the first and repeated adoption. Bytes,
  read-only state, verification, Manual registry and untouched peer file also
  pass; copy/skip fallback is not accepted. This does not cover all Windows
  filesystem cases or different-volume fallback.
  [Results at 1e0dfc7](https://github.com/678mjoe/JMCLCore/blob/1e0dfc763bf9bce00e8b15b8fc2f4bb203e047ba/rust/compat/windows_adopt/README.md).

These counts describe different evidence layers and must not be summed into a
completion fraction. COMPAT-URL-001 remains the six named accepted construction
and representation differences. A new byte-preserving transport is **not** a
required replacement blocker, and acceptance does not waive unrelated security,
HTTP/TLS/redirect or parser requirements.

## Important remaining scope

| Area | Current classification | Concrete prerequisite / next decision |
|---|---|---|
| Managed Java acquisition (`java.runtime.install`) | Confirmed unimplemented; automatic acquisition/runtime-archive installation remains outside the currently permitted scope. | This is a genuine missing capability plus a scope restriction, not a newly observed test failure. Do not retry/download/extract through another route. Ordinary installed-JVM probes and owned non-Java ZIP fixtures are separate and have already run. |
| Tauri command and UI boundary | The actual frontend crate check and Zig-backed debug packaging already passed. Actual Tauri command/invoke execution with the Rust sidecar, WebView/JS event delivery and mounted UI remain unrun. | A bounded actual-command/MockRuntime contract is source-supported on existing public runners, as detailed below. Its missing test-only harness is not a permission blocker. Real WebView and mounted-UI acceptance needs a separately selected native Tauri runtime/display setup; no such Rust-sidecar run is claimed. |
| Actual release package | Release-profile resolver probe passed; actual bundled Rust sidecar placement, installer behavior, signing/updater and release-profile Core package did not run. Existing frontend packaging still pins Zig. | First select one owned unsigned Rust-sidecar package and native install/launch path for review. No production pin switch, signing or release follows from adjacent-file discovery. |
| Live authentication and credential lifecycle | Synthetic account metadata/auth transport is not a real login, refresh-token, keychain or credential-injection result. | Needs an explicitly authorized test account/service and secure credential flow, plus a bounded login/refresh/logout plan. This is an unmet test prerequisite, not evidence that the implementation failed. |
| Real provider, install and loader/game flow | Synthetic cached install, modpack/world archives, parser and URL construction checks do not prove live artifact acquisition, TLS/redirect/origin behavior, authentic game assets, loader processors or gameplay. | First review an owned install/prepare flow with pinned benign or verified fixtures; use an explicitly selected existing runtime where a JVM is needed. Identify the provider endpoints/assets and bounded processor inputs; account operations need the authorized account. Automatic Java acquisition stays excluded; ordinary archive fixtures are not automatically prohibited. |
| Remaining native business/file contracts | Four-platform coverage is specific to the two wrapper/six Session and discovery lanes; Windows original-file identity is one exact test on its own tree. | Review an actual unverified normal flow or concrete defect before adding a new slice. Windows cross-volume fallback, arbitrary filesystem behavior, broader install/launch and live server/SSH are not inferred from these results. If a combined candidate is proposed, explicitly integrate the selected test/harness changes and validate that exact tree; equal production subtrees alone do not do so. |

### Next executable main contract: actual Tauri command boundary

This is a proposed next slice, not an executed result or CI authorization.
Read-only review of the pinned frontend's
[`lib.rs`](https://github.com/678mjoe/JMCL-frontend/blob/9c0fd81ce8109c1e35229560fee31308ca114ef5/src-tauri/src/lib.rs)
and `pool.rs` finds a bounded actual-command test possible without starting the
application's `run()` function, plugins, a real WebView, or any real service:

1. Add one opt-in test-only entry within the real `src-tauri` crate so it calls
   existing private `core_open`, `core_request` and `core_close`, not copied
   wrappers. Use an owned temporary root for `SessionPool` and diagnostics,
   the explicit frozen Rust Core binary path, and default activation state.
2. Exercise open/handshake, a ping, error survival and close through those
   commands; assert closed-session behavior and cleanup of the owned child.
   Keep a reviewed RPC allowlist, `curseforge=false`, and no credential,
   provider, install, Java, launch, updater or SSH operations.
3. The lockfile pins Tauri 2.12.5. Its official
   [test utilities](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/test/mod.rs)
   expose `mock_builder`, `mock_context` and `get_ipc_response`; `tauri/test`
   enables that module. A test-only feature can provide state through
   `MockRuntime`, and `Channel::new` provides an in-memory callback. Direct
   function calls accept only the actual-command layer. An additional generated
   `generate_handler!`/Invoke-JSON path can test argument decoding and dispatch;
   a registered subset must be checked against the production registration and
   must not be called an executed full application registration table.
4. Before execution, review the exact harness, pin both repositories, add an
   external bounded deadline and owned-child cleanup on failure. Tauri's
   `get_ipc_response` itself waits without a timeout. Compile/link dependencies
   still apply: the existing Linux job installs WebKitGTK and related native
   libraries, and `features=[]` does not disable Tauri's default Wry dependency.
   Existing public native runners have built this crate; this proposed test
   harness has not yet been built or run there.

This advances the missing application command boundary instead of repeating
standalone Session tests. MockRuntime does not establish JS/WebView IPC transport,
JS Channel delivery, mounted account/content/launch UI, window lifecycle or
installed-package behavior. Those later checks require a real Tauri/WebView
runtime and suitable native display/automation setup. No Chromium attempt or
workaround for its earlier local permission error is part of this plan.

### What is actually blocked or missing

Missing automatic Java acquisition is a confirmed implementation gap whose
acquisition/runtime-archive execution remains outside the permitted scope.
Real authentication needs an authorized account/service and secure credential
flow. A real game/provider/loader acceptance run still needs selected artifacts,
endpoints and bounded execution inputs; this is not evidence of a failed
implementation or a blanket prohibition on synthetic archives/installed JVMs.
The proposed Tauri command harness and remaining finite native business flows
are **not yet implemented/executed**, not permission-denied. No new tool denial
was observed while checking these completed runs. This documentation review did
not run tests, start CI, merge separate experiment trees, or change a production
pin.

COMPAT-URL-001 does not add a raw-byte transport blocker. Protocol-v1 live task
cancellation remains a separate post-baseline feature, not an unfulfilled
existing cancel-method contract. Neither is a reason to reopen already accepted
scope or manufacture another compatibility count.

No tool action was denied during the documented slices. The missing runtime
acquisition capability, a deliberately excluded operation, a required GUI/test
account prerequisite and an unrun acceptance case are different states; none
should be misreported as a tool denial or a permanent ban. The synthetic tests do not
remove the listed prerequisites or authorize merging/releasing the candidate.

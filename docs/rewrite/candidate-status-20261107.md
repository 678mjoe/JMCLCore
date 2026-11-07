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

## Later Tauri acceptance: three separate Linux test trees

These three Linux x86_64 runs each executed exactly one selected test: 1 passed,
0 failed, 0 ignored, 83 filtered. They retain frontend baseline `9c0fd81` and
Core `bdd6cb3e` with the full hashes above. Production function bodies, original
capabilities and dependency versions remain unchanged. The frontend test
additions are **three different overlays**, not one integrated tested tree;
none extends the earlier four-platform result to Tauri runtime acceptance.

- **Original command/invoke lifecycle through MockRuntime:**
  [run 42396085950](https://github.com/678mjoe/JMCL-frontend/actions/runs/42396085950),
  execution `cec7a6f816946beb76d4f0fa82e999bf730967d5`, tree
  `995c08090a9a3fdab6af028faa537cada35343d1`.
  Original generated `core_open`/`core_request`/`core_close` wrappers opened two
  real RustCore sessions; ping, unknown-method recovery, isolated/idempotent
  close and empty-pool cleanup passed. Requests use the actual mock window URL;
  the original incorrect remote HTTP origin remains a required ACL rejection.
  [Results at 3fa0f4e](https://github.com/678mjoe/JMCL-frontend/blob/3fa0f4ea750330c8b1fc301d4c9ccfee5f3ff490/TAURI_RUST_CORE_CONTRACT.md).
  This executes generated Invoke dispatch, not just direct Rust function calls,
  but uses a dummy context and no real WebView.
- **Production development context with the same bounded lifecycle:**
  [run 42398397584](https://github.com/678mjoe/JMCL-frontend/actions/runs/42398397584),
  execution `fec042a709774b4d3558f3609f853c53ce75158b`, tree
  `29da13cbd401c43c660d89b1736433e54176b2e8`.
  Original `generate_context!()` reads production config/capabilities; the actual
  window URL equals unchanged `http://localhost:1420/`. The remote-origin
  negative control and both real Core sessions pass without expanding ACLs.
  [Results at cbff5e1](https://github.com/678mjoe/JMCL-frontend/blob/cbff5e1c2f79ddb6a964f72af4458fe9c81b694d/TAURI_PRODUCTION_CONTEXT_CONTRACT.md).
  This is **development**, not release/custom-protocol context. The build-only
  `externalBin=[]` override excludes Zig sidecar staging; complete production
  plugin/setup execution and an unmodified packaging environment are not tested.
- **Network-denied Rust Channel serialization outlet:**
  [run 42399164806 / job 128482992387](https://github.com/678mjoe/JMCL-frontend/actions/runs/42399164806/job/128482992387),
  execution `bcf7efbfac0843c3f5ae3e0aed4c859f32857bdc`, tree
  `f1367e2c0bb1f7b548610e343fbc3f934e062ead`.
  The unchanged pinned guard (SHA256
  `2790712aaf757536329cc1ac87c73931f04bf1a1f6dc3f375f782db08423e719`)
  passed its complete selftest and both original native spawn gates. The entire
  Tauri inventory and exact test executable then ran under that guard; build
  preparation is outside it. With one owned mrpack and 35-byte prefilled CAS,
  the actual command/Session/Core path reached official Channel decoding,
  serialization and its Rust interceptor: callback 731, indices 0/1, request
  `req-1`, cached file with attempts 0, then progress 1/1 and transfer bytes 0.
  Terminal result, CAS and installed bytes matched. All 13 phases completed
  without timeout and with empty owned groups; that is not a per-child PID ledger.
  The test log is 2,441 bytes, SHA256
  `e8107473a39060647aadf21d6d3bba286054f40c89d34fb9db35edcde0c0ce47`.
  [Results and raw evidence at 12a4125](https://github.com/678mjoe/JMCL-frontend/blob/12a4125e3fc3369d9b93015238be54cfbde2fb32/docs/acceptance/tauri-channel-egress-20261107/RESULTS.md)
  are a later docs-only commit whose sole parent is the execution commit.
  This uses `mock_context`, **not** the preceding production-context overlay.
  It proves neither JS delivery nor Channel end/unregister, real cache-miss
  integration, real WebView/UI, complete setup or a shipping package.

These counts describe different evidence layers and must not be summed into a
completion fraction. COMPAT-URL-001 remains the six named accepted construction
and representation differences. A new byte-preserving transport is **not** a
required replacement blocker, and acceptance does not waive unrelated security,
HTTP/TLS/redirect or parser requirements.

## Important remaining scope

| Area | Current classification | Concrete prerequisite / next decision |
|---|---|---|
| Managed Java acquisition (`java.runtime.install`) | Confirmed unimplemented; automatic acquisition/runtime-archive installation remains outside the currently permitted scope. | This is a genuine missing capability plus a scope restriction, not a newly observed test failure. Do not retry/download/extract through another route. Ordinary installed-JVM probes and owned non-Java ZIP fixtures are separate and have already run. |
| Real Tauri runtime and UI | Actual generated command dispatch, production development context and Rust Channel outlet each passed their separate Linux MockRuntime slice. Real WebView/JS delivery, mounted UI, complete plugin/setup and release/custom-protocol context remain unrun. | Review one real WebView/JS-to-Core-to-Channel flow with owned synthetic data, a suitable native runtime/display and bounded cleanup. Its harness/executor have not been selected; this is not a new permission denial. Do not repeat the now-passed command-only plan or combine the three overlays without exact-tree validation. |
| Actual release package | Release-profile resolver probe passed; actual bundled Rust sidecar placement, installer behavior, signing/updater and release-profile Core package did not run. Existing frontend packaging still pins Zig. | First select one owned unsigned Rust-sidecar package and native install/launch path for review. No production pin switch, signing or release follows from adjacent-file discovery. |
| Live authentication and credential lifecycle | Synthetic account metadata/auth transport is not a real login, refresh-token, keychain or credential-injection result. | Needs an explicitly authorized test account/service and secure credential flow, plus a bounded login/refresh/logout plan. This is an unmet test prerequisite, not evidence that the implementation failed. |
| Real provider, install and loader/game flow | Synthetic cached install, modpack/world archives, parser and URL construction checks do not prove live artifact acquisition, TLS/redirect/origin behavior, authentic game assets, loader processors or gameplay. | First review an owned install/prepare flow with pinned benign or verified fixtures; use an explicitly selected existing runtime where a JVM is needed. Identify the provider endpoints/assets and bounded processor inputs; account operations need the authorized account. Automatic Java acquisition stays excluded; ordinary archive fixtures are not automatically prohibited. |
| Remaining native business/file contracts | Four-platform coverage is specific to the two wrapper/six Session and discovery lanes; Windows original-file identity is one exact test on its own tree. | Review an actual unverified normal flow or concrete defect before adding a new slice. Windows cross-volume fallback, arbitrary filesystem behavior, broader install/launch and live server/SSH are not inferred from these results. If a combined candidate is proposed, explicitly integrate the selected test/harness changes and validate that exact tree; equal production subtrees alone do not do so. |

### Next main-flow preparation, not another mock acceptance claim

The previous proposed command-only harness is now implemented and passed in the
three bounded layers above. The next useful frontend boundary is an actual
WebView JavaScript invoke and Channel receiver around owned synthetic data,
followed by a mounted account/content UI flow. Its scope must identify the real
Tauri/WebView runtime, display/automation, exact frontend/Core overlay, expected
JS messages and terminal result, and bounded child/window cleanup. Compilation
and MockRuntime do not establish those runtime behaviors. No such new harness
or run is authorized or executed by this documentation update.

The local worker lacks native GTK/WebKit development packages; this is a local
build prerequisite, not a claim that standard runners cannot host a future
reviewed slice. Those runners have built the real crate, but no real
WebView/display/automation arrangement has yet been verified for this candidate.
The earlier local Chromium permission error is not retried or worked around.
A future native WebView proposal requires its own review, not an inference of
permission from successful mock tests.

An unsigned Rust-sidecar package is another concrete unrun main flow: first
select the intended Core build profile/target and owned package layout, then
review staging, inspection and bounded native launch. Existing Zig packages and
the release resolver probe do not satisfy it. A production pin switch, user
machine installation, signing, publishing or updater rollout is outside these
acceptance results and must not happen implicitly.

### Actual restrictions versus unmet prerequisites

- Managed Java acquisition is genuinely unimplemented, and its automatic
  acquisition/runtime-archive execution remains restricted. Do not fetch or
  extract a runtime through another route. Installed-JVM probes and owned
  non-Java archives remain distinct permitted activities.
- Real authentication/refresh/keychain acceptance needs an authorized account,
  service and secure credential flow; synthetic identities do not supply them.
  Do not obtain or persist credentials merely to fill the matrix.
- Real provider, loader and game acceptance needs selected endpoints/artifacts,
  bounded processor/game inputs and an appropriate existing runtime. This is an
  unmet acceptance plan/input prerequisite, not a blanket ban on preparing
  owned fixtures or evidence of a failed implementation.

Real WebView/UI, complete setup, Rust packaging and remaining finite native
business flows are **unrun or not yet designed**, not automatically permission-
blocked. Existing access restrictions still apply to any proposed action.
The retained source reports distinguish original rejected attempts from later
successful bounded runs. This documentation review itself did not execute
Tauri/Core/tests, start CI, merge experiment trees, or change a production pin.

COMPAT-URL-001 does not add a raw-byte transport blocker. Protocol-v1 live task
cancellation remains a separate post-baseline feature, not an unfulfilled
existing cancel-method contract. Neither is a reason to reopen already accepted
scope or manufacture another compatibility count.

The missing runtime acquisition capability, an excluded operation, an unmet
runtime/account prerequisite and an unrun acceptance case are different states.
The synthetic tests do not remove these prerequisites or authorize merging or
releasing the candidate.

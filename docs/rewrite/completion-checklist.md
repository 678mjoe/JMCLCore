# Bounded rewrite completion checklist

This checklist separates **implemented names**, **finite contract evidence**, and **replacement readiness**. It is not permission to replace Zig `main`. All integration goes to `rust-rewrite`; `main` remains the pinned Zig project.

## Published checkpoints

- [x] Milestone 1: PR #3 squash `c058566815e10beb0bdd0e1142cd895bc362b2c4`; exact-head and postmerge Rust/Zig checks passed on Linux, Windows, macOS ARM and Intel.
- [x] Milestone 2: PR #4 squash `e95a1906a4d9dd12cc54b6cd8210bfd56c4501ad`; exact-head native checks passed on all four platforms. This includes the direct-exec macOS correction, not a normalized error result.
- [x] Milestone 2 postmerge push runs `40082466174` / `40082466203`: all eight native jobs reached terminal success, verified 2026-10-24 09:53 UTC.
- [ ] Milestone 3 remains a separate frozen integration scope, with the prerequisite macOS fix carried forward. Its own exact-tree native checks and postmerge checks are still required.
- [ ] Milestone 4 combines later server/provider/auth/processor/launch and numeric work. Individual module tests are not a substitute for the combined exact-tree suite or native CI.

## Concrete ordinary-contract work still open

1. **Default instance creation validation — implemented, combined gate pending.** RPC and CLI now resolve cached metadata before opening the root, including all supported loader/source selections. The isolated 71-case corpus checks exact output/status/disk effects, failure precedence and explicit no-validation behavior. The combined exact-tree/native run remains required.
2. **Metadata numeric lexemes — implemented, combined gate pending.** A private typed AST retains raw numeric lexemes and uses the shared binary128 conversion without changing dynamic RPC semantics. Sixteen added pinned-oracle cases plus the previous metadata corpus passed in isolation; combined/native acceptance remains required.
3. **World CRC/named-alias compatibility — implemented, combined gate pending.** Stored and deflated entries now preserve baseline CRC-only acceptance under existing path, encryption, decompression and size checks. Explicit existing world-directory aliases and icon/session-lock aliases follow the original selected-target contract; recursive copy/export still reject symlink entries and archive staging confinement remains unchanged. This is ordinary compatibility, not an approved-defect exception. The combined/native corpus remains required.
4. **Byte paths and remaining error corpora.** Unix raw installer/override paths and processor arguments have specific tests, but content metadata filenames, managed-runtime-store lookup and every native path variant are not universally covered. Do not extrapolate those tests into complete non-UTF-8/Windows path compatibility.
5. **Combined dispatch/CLI/receipt audit.** Reconcile all 106 original names against the actual dispatcher and the per-method matrix, including required parameter branches and Linux-only RPC versus portable CLI asymmetry. Names alone are not completion. Run the committed negative, event, disk-takeover and receipt tests explicitly, not as ignored successes.

These are bounded known gates, not a request to add unrelated features, speculative redesigns or unlimited fuzzing.

## Blocked runtime acquisition

`java.runtime.install` and automatic runtime acquisition used by launch/loader/server orchestration remain incomplete. Local or already-installed runtime paths have evidence; `JavaDownloadFailed` for unavailable acquisition is **not** proof of production downloader-failure parity. The owner approved the narrow staging-confinement policy; implementation and verification of runtime installation are still incomplete. Policy approval is not evidence of a working installer. No success receipt may be written for an unavailable stage.

## Approved deviations versus unresolved differences

The owner approved correcting individually source-and-test-proven original defects while preserving normal behavior, and requested a separate original-defect ledger. Stable entries are tracked in `original-core-defects.md`: oversized buffered-request loss, inherited-output-pipe completion, loader response hash lifetime, dynamic numeric conversion abort, observed Windows lock contention behavior, and the narrowly scoped archive safety boundary. Approval and implementation/evidence status must be separate for each entry. A reported inference or unverified platform cause must not be upgraded to a proven defect by association.

Literal wire-equality mode may still expose a documented correction. Compatibility mode must allow only the exact approved case and compare every unaffected byte/event/disk effect; every unexpected mismatch fails. No blanket normalization or permission to redesign interfaces follows from the policy. The original Zig source is not edited by these integration steps.

## Evidence required before replacement can be considered

- Combined pinned Rust formatting, Clippy and all-target tests, with required oracle environment variables and zero silently skipped oracle cases.
- Exact-head native Linux/Windows/macOS ARM/macOS Intel checks, including real file identity/link counts, copy fallback where available, read-only publication, locks, private mutable data and portable CLI tests. Cross-compilation is only an additional syntax/type gate.
- Unmodified pinned Zig oracle for ordinary raw wire/CLI/disk cases; clearly labeled, source-hashed injected transport oracles for synthetic service cases. No injected oracle may be described as live transport acceptance.
- Explicit unresolved live HTTP/TLS/redirect/provider error behavior, actual authentication/service behavior and untested platform variants. No credentials, game/server deployment or real account tests are implied by synthetic tests.
- [Unchanged-frontend bridge smoke](frontend-bridge-evidence.md) passed at the recorded historical Rust checkpoint: 19 native wire requests and 11 TypeScript wrapper requests against each core, with two pre-existing wrapper mismatches explicitly retained. Later revisions, full native Tauri IPC and packaged/UI acceptance are not covered. No frontend source was changed.
- Exact merged tree verification, unchanged `main`, and terminal postmerge checks for every published milestone.

`replacement_allowed` and `full_compatibility` remain false. Stop expanding scope once these planned gates are covered; if a gate needs unavailable execution capability, credentials or additional authority, report that exact gate rather than claiming completion.

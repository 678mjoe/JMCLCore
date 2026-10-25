# Fourth experimental integration checkpoint

This checkpoint remains on `rust-rewrite`, not Zig `main`. It closes planned implementation slices, not replacement acceptance.

## Exact local checkpoint

- Combined source/document checkpoint: `497a4565c408b0658ac1d450c05d2f04a8f69c03`, tree `8a2654ef5236bafcc7ed0820e275ac541b95dd7c`.
- Rust 1.99.1 formatting and all-target Clippy with warnings denied passed.
- Linux all-target tests explicitly executed required oracle tests: **179 unit + 123 integration test functions passed; zero ignored**. This includes all six CRC metadata combinations (local/central/both × stored/deflated), the bounded accepted-socket fixture correction, positive approved-defect assertions and private mutable-world checks.
- Rust executable SHA-256: `097e5e59e4763890c9f300d97d2ba73342a47639177645e83538633dbe9955ad`.
- Unchanged Zig source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`; executable SHA-256: `b9e628efc5b7ea34047ad25129a98756b6f8ec5d0be38193aee2620d87a82bfe`.

Later documentation-only changes do not change this executable. Implementation/test changes require their own rerun. The optional latest Windows GNU cross-check was stopped before completion; **it is not a Windows pass**. Native four-platform CI remains required for this publication tree.

## Additional finite corpora

These counts describe their own fixtures; do not sum them as unique requirements or a coverage percentage.

| Corpus | Executed result and scope |
|---|---|
| Version metadata/planning | 106 network-free exact RPC/CLI cases; Vanilla/Fabric/NeoForge/three Forge profile families |
| Java detection/selection/receipts | 59 exact RPC/CLI/direct-disk cases; no automatic runtime acquisition |
| Public account metadata | 41 raw RPC cases, bidirectional disk takeover and five corrupt manifests |
| Launch plan | 44 exact offline RPC cases |
| Launch execution/CLI | 34 local/installed-runtime cases; only PID/duration/RSS normalized with separate assertions |
| Instance install | 17 exact CLI/receipt cases, plus hardlink/private-data tests in the suite |
| Default instance validation | 71 cached RPC/CLI cases, exact output/status/disk effects |
| Protocol numeric/disk edges | 153 exact raw cases; typed numeric golden cases are separately included in module tests |
| Store GC | 56 raw RPC/CLI cases, real lock and file-identity tests in the suite |
| Framing | 51 exact cases plus three precisely asserted approved ZIG-001 corrections |
| Linux server lifecycle/worlds | 494 cases; RPC parsed JSON and CLI exact bytes/status, documented backup timestamp normalization |
| RCON/Query | 45 synthetic loopback cases, zero differences |
| Server create/install | 171 cached RPC/CLI/disk cases, zero differences; Vanilla/Fabric end-to-end success, not real game deployment |

Injected auth/provider/processor oracles use explicitly substituted synthetic transports and pinned source. They are not described as unchanged production transport or live service acceptance. Loader RPC corrections require the actual generated SHA-1 and compare every unaffected record/receipt; no arbitrary hash mismatch is accepted.

## Scope and remaining boundaries

The dispatcher inventory records **105 implemented or partially implemented method names out of 106**, with **zero methods claimed universally compatible**. `java.runtime.install` remains unavailable; automatic Java acquisition in launch/install/server orchestration also remains incomplete. Local or installed Java success cannot stand in for that missing pipeline.

[The completion checklist](completion-checklist.md) retains native combined execution, live HTTP/TLS/provider/authentication behavior, non-UTF-8/native-path coverage boundaries and full replacement gates. [The original-defect ledger](original-core-defects.md) separates approved source-proven corrections from ordinary compatibility behavior and source-only forwarding risk. [The frontend bridge smoke](frontend-bridge-evidence.md) is a finite historical check of unchanged modules, not packaged application acceptance.

No release, production account/server, frontend modification, or change to Zig `main` is implied. `full_compatibility=false`; `replacement_allowed=false`.

## Native-fix carry-forward checkpoint

After carrying forward the observed macOS lock-creation correction, native path-length limits, capability-aware raw-name rejection fixtures and exact Unix EILSEQ error mappings, source `0b878035f81041ccd891098a8a0eb27e0012f48b` passed **186 unit + 123 integration test functions (309 total), zero ignored**, plus pinned formatting/all-target Clippy on Linux. Executable SHA-256: `4201b9b01f9bf20521e0cd92f22bb1644901e292ed6d066cf1d8e0e767a800a0`. The unchanged bridge was also rerun at this hash; see its evidence document. Raw-byte filename success is not claimed on filesystems which reject those names: native CI explicitly exercises rejection/error/cleanup instead where possible and reports unavailable success cases. Combined milestone-four native acceptance is still pending, independently of milestone-three checks.

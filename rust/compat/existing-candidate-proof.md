# Existing-runtime receipt proof and bounded selection follow-up

## Current status, 2026-11-09

Existing managed-candidate lookup now shares the validated byte-preserving
receipt view used by runtime listing. This fixes otherwise eligible UTF-8
executable paths being hidden by byte-array metadata or integer-shaped provider
tags. It is a bounded existing-runtime fix, not runtime acquisition, archive
installation or complete managed-selection compatibility.

Opaque executable bytes remain unresolved: listing preserves them, but the
probe API currently accepts UTF-8 paths only. Selection skips that candidate
and continues to later candidates. This does not classify the receipt as invalid.

The original existing_candidate.rs proof remains test-only and is not called
by operational lookup. Its buffer-only dependencies and four tests are unchanged.

## Discrepancy and minimal correction

Previously managed.rs called store::list, which parsed into the legacy
String-based Receipt. Runtime listing used store::list_read and ReadReceipt.
The legacy parser rejected otherwise valid byte-array executable/component/
ID/platform fields, descriptive bytes and integer-shaped provider tags.

Only the existing-candidate branch changes:

- Use the existing list_read view; leave the legacy String API available.
- Filter recorded major and platform as before.
- Compare preferred components by exact bytes, preserving two-pass lookup and
  directory-name order. Newly accepted valid candidates may intentionally change
  which runtime wins in a mixed directory.
- Convert only the executable path to UTF-8 for the existing probe. Skip opaque
  paths without hiding later eligible candidates.

The resolve policy engine, local discovery, downloader callback, probe, parsers
and receipt writer are unchanged. Store and read-view changes are comments only.
Both list variants already create missing directories and clean staging entries;
these side effects are not introduced or removed by this change.

Pinned Zig 5a4310debbcee26b3d2391bab2339f79cd6ec52e remains the source reference:
src/java/java_runtime_store.zig:24-42 and :144-190 define typed byte receipts and
validation; src/java/managed.zig:239-263 filters and probes existing candidates.
This follow-up does not claim a new Zig runtime differential run.

## Bounded regression tests

Five tests in managed_receipt_selection_tests.rs directly call private managed(),
never resolve(), local discovery or a downloader. Stores, receipts and executable
files are owned TempDir fixtures. Existing java_probe.rs is compiled locally into
a native Rust property printer, not a JVM. The probe uses an empty environment.
These are real owned filesystem/probe operations, not pure or read-only tests.

1. listed_receipt_representations_select_the_owned_existing_runtime: ordinary
   receipt plus 13 representation variants; checks listing, exact selected path,
   source, reported host architecture and unchanged receipt bytes. Before the
   fix, provider=0 was listed but not selected; afterward all variants pass.
2. byte_components_preserve_preference_and_sorted_fallback: preferred bytes,
   no/missing component, sorted fallback and major/architecture filters.
3. invalid_receipts_do_not_become_candidates: ordinary validation failures remain
   absent from listing and candidate selection.
4. opaque_executable_remains_a_listed_unresolved_candidate: records the remaining
   UTF-8 probe limit; no opaque executable is created.
5. opaque_executable_does_not_hide_a_later_eligible_candidate: sorted-first opaque
   record does not suppress a later ordinary candidate.

For each NAME:
cargo test --manifest-path rust/Cargo.toml --offline --locked --lib java::managed::receipt_selection_tests::NAME -- --exact --nocapture

The recorded red/green result is Linux x86_64 in an isolated source-slice crate
with exact original modules and locked dependencies. Its wrapper omits the
production dispatcher, launch and installer. This is not a full repository build,
RPC/application acceptance, actual JVM operation or Windows/macOS run. The
wrapper also lacks the Windows store backend and is not a native-matrix substitute.

The first source-slice compile lacked the existing protocol-number test data.
That exact original blob was added before the real failing test executed.
The setup compilation failure and actual red test are retained separately.

## Full-checkout follow-up, 2026-11-09

The same patch was then applied to a clean, complete Git checkout at
bdd6cb3e55d69cb9e0e02037d251635e289207a8. All 20 original source-slice inputs
matched their Git blob identities in that checkout. The checkout used the
real production lib.rs and main.rs, not the isolated wrapper.

On Linux x86_64 with Rust 1.99.1 and offline locked dependencies, the following
passed without warnings:

- cargo check --manifest-path rust/Cargo.toml --offline --locked --lib --bin jmcl-core-rust
- cargo build --manifest-path rust/Cargo.toml --offline --locked --lib --bin jmcl-core-rust
- cargo clippy --manifest-path rust/Cargo.toml --offline --locked --lib --bin jmcl-core-rust -- -D warnings
- The five new selection tests, four original candidate proof tests and seven
  receipt tests, each selected with its exact module prefix and --lib.
- The source guard, its six controls in normal and optimized Python, targeted
  rustfmt checks and git diff --check.

Only 16 of the 485 library tests ran; no integration-test binaries or production
CLI were executed. The produced library/CLI build does not establish application,
native-matrix, actual JVM, runtime-acquisition or archive-installation acceptance.
No installation, download or fallback path was exercised. The full source from
pub fn resolve through the end of managed.rs remained byte-identical to the base.

## Original pure proof and review guard

The four original proof tests still cover in-memory grammar and candidate
filtering without filesystem or process operations. Legacy Receipt rejection
remains the historical contrast; opaque executable bytes remain unresolved.

check_existing_candidate_proof.py pins the reviewed source and requires one
test-only inclusion of the proof. The November 9 update re-pins the reviewed
selection change and two comment-only dependencies. It does not wire the proof
into production, remove a check or turn source hashes into a runtime proof.
The six mutation/line-ending controls are unchanged.

Run the source guard and controls in normal and optimized Python:
python3 rust/compat/check_existing_candidate_proof.py
python3 rust/compat/test_existing_candidate_proof_guard.py
python3 -O rust/compat/test_existing_candidate_proof_guard.py

Remaining work includes opaque executable paths, full mixed-store semantics,
broader acceptance and native integration. The old behavior of trusting the
receipt's major/platform and then returning a successful probe is unchanged;
this is not revalidation of every reported JVM property. Automatic Java
acquisition and runtime-archive installation remain unimplemented and unverified.

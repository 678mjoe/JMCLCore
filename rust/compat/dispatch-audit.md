# Static RPC routing and evidence-reference audit

Reviewed baseline: `c0bda37fb66366123f3159fd0d9f99a00bd000b9`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

The existing `validate_coverage.py` checked the original Zig dispatch names
against inventories. It did not associate those names with Rust handlers. The
new `dispatch-audit.json` records a **manual source review**, checked for drift:

- The ordered calls in `rust/src/protocol.rs` lead to streaming, source-result,
  generic-result, builtin and inventory-fallback branches.
- Every inventoried name has exactly one reviewed association. Literal guards
  are recorded directly; content families use explicit family × operation
  expansions, including their `Kind::from_directory` dependency.
- Re-exports and nested server dispatch links are recorded rather than treating
  an identical symbol name in another module as the implementation.
- Server RPC handlers reject non-Linux targets. This says nothing about the
  separate portable server CLI surface or its acceptance evidence.
- `java.runtime.install` is **blocked and not implemented**: the reviewed
  dispatcher reaches its inventory-only `NOT_IMPLEMENTED` fallback. It has no
  runtime-evidence reference and is never invoked by this audit.

The current result is 106 inventoried names, 105 static handler associations,
and one explicitly blocked fallback. These are **classification counts, not a
completion fraction**. A handler association does not prove successful operation,
complete parameters, error precedence, events, receipts, filesystem effects,
platform equivalence or automatic Java acquisition. In particular install,
launch and server orchestration still lack automatic runtime acquisition.

## What the validator proves and does not prove

The validator checks source SHA-256 fingerprints (CRLF line endings canonicalized to LF), source call/re-export/selector
anchors, ordering, method literals or finite product expansion, complete/disjoint
inventory membership, platform labels and existence of related fixture sources.
It fails closed when a pinned source changes. It is **not a Rust semantic parser**
and does not rediscover routes automatically. Correctness of the initial route
association depends on the recorded source review. Anchors alone are not proof:
for example a quoted method in a diagnostic is not necessarily a dispatch guard.
Fingerprints bind the review to the complete source files and prevent silently
reusing it after a guard, order, re-export or implementation change.

`runtime_evidence.status = referenced_not_run` means only that related group
fixture source exists. It is not a per-method passing test, not a test run record,
and not new native/live acceptance. The source references include fixtures that
are **not executed by this static audit**; references do not establish their
runtime results. No Core RPC/CLI requests, Java binaries, installer/archive
operations, network providers, accounts or servers are executed by this audit.

Use the existing per-method evidence matrix and exact-commit CI records for
finite executed evidence. This change does not promote stale matrix claims,
clear their gaps, or set `full_compatibility` / `replacement_allowed` true.

## Safe checks

From the repository root:

```sh
python rust/compat/test_validate_coverage.py
python -O rust/compat/test_validate_coverage.py
python rust/compat/validate_coverage.py --static-only
python rust/compat/validate_coverage.py --oracle-source /path/to/pinned-zig-source
```

The last command also performs the existing inventory and original revision
checks; it invokes only `git rev-parse`, not the original executable. The
`--static-only` mode does not even invoke git. The normal workflow's existing
validator call now includes the static audit. The 22 focused regression tests
cover source drift, absent handlers/calls, route ordering, missing/duplicate
methods, wrong product expansion, unpinned selectors, false fallback/evidence
associations, fixture-reference failures, path escape and Linux RPC boundaries.
They passed locally in normal and optimized Python modes. This is not a claim
that the full Rust/native test suite was rerun for this documentation/tooling
change.

## Integrating subsequent changes

If a pinned file changes, review the changed routing guard, nested call and
re-export associations before updating its digest. A non-routing edit can also
trigger this conservative gate; inspect it, record the new reviewed revision,
and refresh only the reviewed digest. **Do not add an automatic repin switch or
blanket-ignore source drift.** Run all four safe checks above on the final
integrated tree. Concurrent fixes should be sequenced by the integration owner.

Further executable contract work remains separate: current-frontend bridge
fixtures, ordinary parameter/error matrices, native Java detect/select and byte
path/disk-takeover gaps. This static audit is useful preparation, not a substitute
for those bounded tests and not permission to run a blocked operation.

## Checkout line endings

The source digest encoding is explicitly `utf8_crlf_to_lf`: only paired CRLF
bytes are changed to LF before fingerprinting and source-anchor checks. Git text
checkout conversion must not invalidate an otherwise byte-identical reviewed
source. Bare carriage returns, spaces, Unicode, escape sequences and changed
routing tokens are not normalized. The validator reports exactly which files
required CRLF canonicalization; all still must match the reviewed fingerprint.
Controlled `core.autocrlf=true` checkout reproduces the original Windows-style
15 failures / one error, while its normalized source equals the reviewed Git
blob. New positive CRLF and negative changed-token/bare-CR/policy tests preserve
the fail-closed boundary. Native CI confirms the actual runner checkout result.

## Unix probe integration review

The follow-on Unix probe change widens only the private `direct_exec` module
compile condition from macOS/tests to Unix. Manual review of `java/mod.rs`
confirmed its method guards, handler calls, parameter dispatch and runtime-install
fallback are unchanged; its single source fingerprint was updated accordingly.
The explicit-probe executable lookup regression is separate runtime evidence,
not a change to the 106-name routing inventory.

## Typed string-array parser integration review

Manual review of the registry parser correction confirmed that `Kind` variants,
`Kind::from_directory` and content family routing anchors are unchanged. Only
string deserialization and ordered persisted-source validation changed. The
registry source fingerprint was refreshed after this review; the route map and
all 106 inventory classifications remain unchanged.

## Read-only byte-path listing review

Manual review of the byte-preserving listing view confirmed unchanged method
selection and content-kind mapping in `content/mod.rs` and `registry.rs`. The
new datapack-list error guards affect source-compatible error classification,
not routing membership. Read-only list records now preserve bytes, while the
mutation/adoption and save types remain String-specialized. Only these two
reviewed source fingerprints were updated; classifications are unchanged.

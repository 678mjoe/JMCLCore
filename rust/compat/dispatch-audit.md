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

## Opaque adoption integration review

Manual review confirmed unchanged adoption method selection and store module
exports. Adoption now preserves listed bytes through the existing locked CAS
primitives; additive content byte-parent wrappers do not alter dispatcher routes
or existing archive/native callers. The content root, adoption and store module
fingerprints were refreshed after reviewing these changes. Inventory membership
and the blocked acquisition fallback remain unchanged.

## Local raw-record mutation review

Manual review confirmed unchanged client and server method selection, parameter
guards and platform gating. Existing remove/toggle calls now select the byte-aware
registry view and retain existing filesystem operations and public String wrappers.
Only the content root and server facade source fingerprints were refreshed; route
membership and acquisition fallback classifications are unchanged.

## Dynamic file-selector integration review

Manual review confirmed unchanged method families, platform gates and operation
selection. Only local file-selector decoding now preserves byte arrays with the
existing dynamic-number helper; other parameter decoders and public String
wrappers remain in place. The byte path validator visibility changed without
changing its rules. The three affected source fingerprints were refreshed after
review; the inventory still does not assert executable compatibility.

## Native local file-argument review

Manual review of the executable pre-dispatch hook confirmed interception only for
existing local content mutation routes with non-UTF-8 `--file` values. All-UTF-8
and unrelated routes remain on the existing dispatcher, including hidden server
lifecycle commands. RPC method guards and membership did not change. The content
root and server facade fingerprints were refreshed; broader native arguments are
not covered by this static RPC inventory or this narrow CLI implementation.

## Account metadata byte-parser review

Manual review confirmed the same four account method guards and handler order.
Private disk and parameter decoders now retain their distinct typed/dynamic byte
grammars; parameter validation still follows the existing empty/root/metadata
order. Public output models and routing membership are unchanged. Only the
reviewed accounts source fingerprint was refreshed.

## Persisted manifest diagnostic and enum review

Manual review confirmed unchanged RPC membership and parameter routing. A private
server show diagnostic channel retains the original anchored load error without
rereading disk; ordinary RPC envelopes are unchanged. Instance and server source
enums now validate during typed parsing while retaining their public String API.
The instance module and server RPC fingerprints were refreshed; these parser and
CLI changes do not expand the static method inventory claim.

The subsequent persisted invalid-ID correction only changes `instance.get`
load-error classification to its source contract. Request validation, delete
handling and method selection are unchanged; the instance fingerprint was
reviewed again after this narrow mapping correction.

## Display-name read-view integration review

Manual review confirmed that only instance/server get/list and CLI show/list
select the byte-aware display-name read view. Other operations still use the
existing operational String manifests. Server get is handled earlier within
the same guarded handler, with unchanged root, platform and error checks; the
installation identity excludes names without fabricating placeholder values.
The instance and server RPC fingerprints were reviewed and refreshed.

## Read-only lookup parameter review

Manual review confirmed dedicated instance/server get and server list parameter
views retain platform gates and method membership while preserving dynamic byte
IDs and ignored values. Root opening still precedes ID semantic lookup. Shared
create/delete/lifecycle parameter decoding is unchanged; outer duplicate-field
rejection remains before dispatch. The two affected fingerprints were refreshed.

## Adoption target decoder review

The existing content-family adoption dispatch and `handle` entry point are
unchanged. Manual review found changes only to byte-string parameter decoding,
ordered native root opening and world-result representation. The per-file
adoption transaction and shared store implementation are unchanged. The source
fingerprint was refreshed for this reviewed decoder change; this does not
classify the existing persistent digest-lock layout difference as parity.

## Server local mutation target review

The Linux server-content method inventory and public dispatcher guard remain
unchanged. Existing enable/disable/remove methods now take a dedicated byte-target
path before the unchanged network/adoption parser. Manual review verified the
root, operational manifest, stopped guard, properties and selector ordering; the
shared local mutation field executor retains CLI behavior. Lifecycle/store
implementations are unchanged. The reviewed server-content fingerprint is
updated; no running-server or non-Linux RPC acceptance is implied.

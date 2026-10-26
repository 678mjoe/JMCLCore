# Existing-runtime receipt eligibility: pure proof only

**Not a production fix; managed-selection parity remains open.** This module is
compiled only under `cfg(test)`. No operational caller is rewired, no candidate
store is enumerated, and no executable is probed. It does not exercise or change
acquisition, fallback downloads, archive handling, runtime plans or receipt writes.

## Source-backed discrepancy

At the reviewed baseline, `java/managed.rs` calls `store::list`, whose documents
use `receipt::parse` and the public String-based `Receipt`. The runtime-list
response instead uses the separate byte-preserving `ReadReceipt` parser.
Thus the old operational parser rejects otherwise valid persisted receipt
representations before candidate filtering: ASCII byte arrays in executable,
component, ID or platform; descriptive byte arrays; and integer-shaped provider
enum tags. Numeric receipt helpers are already shared; this is not a new numeric
precision claim. `java.select` RPC uses local detection rather than this managed
receipt path and is not claimed fixed by this proof.

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e` provides the expectations:

- `src/java/java_runtime_store.zig:24–42`: typed receipt byte slices and enum;
- `:144–165`: validation of ID/platform/major, relative executable/home and URL;
- `:168–190`: completed typed receipt parsing and validation;
- `src/java/managed.zig:239–263`: major/platform/architecture checks, preferred
  component pass, then executable-path construction and probing.

The tests assert actual old `receipt::parse` rejection, not just success of a
new helper. Positive grammar expectations reuse the already independently
exercised listing parser and are source-derived; these tests do not run a new
Zig candidate/probe oracle. The filter checks exact component bytes and the
preferred/fallback pass distinction. It does not establish whole-list ordering
or successful execution. Opaque executable bytes produce an explicitly unresolved
proof outcome, not a silent invalid-receipt or not-found classification.

## Allowed dependency boundary

`existing_candidate.rs` imports only `receipt`. Its input is an in-memory JSON
buffer; its output is skip, a checked UTF-8 executable path, or explicitly
unresolved executable bytes. It calls `parse_read` and `platform_target`, whose
reviewed dependencies are typed JSON/string/numeric validation only. Tests also
call old `receipt::parse` for the negative baseline. There are no calls to
`store::list`, `managed::resolve`, process probing, filesystem enumeration,
network transport or an acquisition callback.

`check_existing_candidate_proof.py` checks the exact reviewed proof digest,
its sole test-only inclusion and unchanged operational managed source after
removing that inclusion. It pins the receipt/store/runtime-plan/probe/select
and JSON helper source files. This is a digest-bound source-review guard, **not**
a general Rust semantic call-graph analyzer. Text hashes normalize CRLF to LF;
six in-memory controls require identical acceptance for native line endings and
rejection of proof, operational, dependency or non-test-wiring changes. Future
edits require renewed review.

Run only these four pure tests, each with the exact-name flag:

```sh
python rust/compat/check_existing_candidate_proof.py
python rust/compat/test_existing_candidate_proof_guard.py
for name in \
  buffer_grammar_accepts_source_receipts_without_lookup \
  candidate_filters_preserve_two_pass_matching \
  invalid_buffers_never_become_candidates \
  opaque_executable_is_explicitly_unresolved
do
  cargo test --manifest-path rust/Cargo.toml --locked --lib \
    "java::managed::existing_candidate::$name" -- --exact --nocapture
done
```

Actual managed selection remains unchanged. Any future operational wiring or
selection execution requires a separate boundary review. Existing store listing
also creates missing directories and cleans staging entries; it must not be
represented as inherently read-only. Neither those operations nor any probe were
used for this proof.

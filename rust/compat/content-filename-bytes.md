# Provider filename bytes through ordinary content installation

This slice preserves provider filenames as bytes through ordinary content
resolution, cached or injected artifact execution, registry publication, RPC
records, CLI output, and version switching. It does not change archive handling,
Java acquisition, HTTP policy, URL representation, or cancellation.

## Representation and storage boundary

Public String-based metadata, plan, installer and content APIs remain available.
Their filename conversion occurs during typed parsing, including its original
error precedence. Internal filename-specialized records share the same provider
validation, selection and dependency logic; their filename representation is
`MetadataBytes`. Filename validation examines bytes, not replacement characters.
All URL fields remain String and all new transport fixtures use ASCII HTTPS URLs.

The ordinary content facade uses an additive `ByteFile` execution view rather
than changing the public installation plan. Its events and saved paths serialize
valid UTF-8 as JSON strings and other bytes as integer arrays. Text CLI output
writes the original bytes. No synthetic placeholder or lossy filename is used.

The executor shares scheduling, size/hash checking, retry and publication logic
with the String engine. `verify_bytes` resolves existing parents without creating
them. Imports and materialization use the existing digest-locked, handle-relative
CAS methods; CAS parents retain no-follow checks. Existing content named-parent
alias behavior and the existing cross-device fallback are unchanged; no new
fallback is introduced. Same-volume fixtures require actual hardlink identity,
not equal content alone. Unrelated shared objects retain their bytes and readonly
state.

Version switching retains source order: materialize the new object, preserve a
disabled filename when necessary, save the updated registry, then remove the old
entry. Callback failures are not advertised as rollback: rejection of a final
file event can leave a correctly materialized object without a new registry.

## Reproducible, network-disabled oracle

`build_content_fixture_oracle.py --deny-artifact-fetch` copies pinned Zig
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`, injects dictionary-only metadata,
and replaces the copied ordinary artifact `downloadOnce` body with
`FixtureArtifactFetchForbidden`. The default builder behavior is unchanged.
The original checkout is not patched. `provenance.json` records the original and
injected file hashes, both patches, the guard flag and output binary hash.

```sh
python rust/compat/build_content_fixture_oracle.py \
  --source "$JMCL_ZIG_SOURCE" --output .filename-oracle \
  --zig zig --deny-artifact-fetch
JMCL_CONTENT_FILENAME_ORACLE="$PWD/.filename-oracle/jmcl-content-fixture-oracle" \
  cargo test --manifest-path rust/Cargo.toml --locked \
  --test content_filename_bytes -- --include-ignored --nocapture
```

Use the `.exe` binary suffix on Windows. The new corpus requires this separate
oracle variable and verifies the pinned revision, guard flag and patch marker.
It does not silently use the default service adapter. Rust cached fixtures panic
if artifact transport is called. Positive cases additionally require cached,
hard-link events, exact registry bytes and immutable object identity.

## Finite evidence

The Linux corpus covers 32 cached client RPC pairs, 24 client/server CLI pairs,
raw and Unicode enabled version-switch CLI/RPC sequences, disabled switch and
unchanged-repeat state, hash failure with an injected bounded body, final writer
failure, and retained String parser error precedence. Families are mods,
resourcepacks, shaderpacks and datapacks; server scope is mods/datapacks on Linux.
The baseline String implementation fails the raw-filename case; the byte path
implementation passes it. Existing ordinary content wire/CLI fixtures remain
strict comparisons. Five focused engine tests separately cover raw events,
String equivalence, retries, hash/writer failures, preflight, noncreating lookup
and CAS parent alias rejection.

Native Windows/macOS execution of this new corpus is still an acceptance gate,
not inferred from Linux results. Filesystems rejecting these physical names
must produce the observed EILSEQ capability outcome, retain Unicode coverage,
and compare deterministic rejection and registry absence. Windows uses valid
WTF-8 lone-surrogate fixtures, not arbitrary invalid native byte paths. Its
already documented ZIG-007 readonly-rename defect is not broadened here. Both
subjects execute the new disabled-switch cases. Success requires strict
event/registry equality plus Rust's positive disabled/shared-inode invariants.
Any source failure records exact events, registry bytes and all old/new filename
states, then fails after the bounded matrix; it is diagnostic evidence, not a
new correction allowance. Native observation/review is required before any
case-specific ZIG-007 gate can be added.

Remaining separate boundaries include non-UTF-8 URLs and broader raw CLI/root
parameters. Passing opaque artifact fixtures is not live-provider, game,
archive, account or production-network acceptance.

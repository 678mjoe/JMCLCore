# Rebuilt modpack artifact path bytes

This is a fresh reimplementation from published baseline
`a4ad20ac62415397cbe6495ce238bbbcef61e2ad`, not recovered old code or evidence.
The unchanged Zig oracle is `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

MRPack `File.path` and CurseForge provider `fileName` now retain typed bytes.
Path validation remains at the original schema position, including NUL errors,
byte length, reserved basenames and validation-before-skip/duplicate ordering.
Kind-prefix and duplicate matching use complete bytes. URLs and slugs are not
changed by this slice.

Modpack-specific File/Plan types preserve all prior fields: usage, URL/fallback,
hash, size, version, source, target, classpath, native extractions, asset
projections, Java major version and loader. The existing general plan types are
unchanged. The service still creates the instance before planning; total size
checked addition precedes Target::host; execution precedes registry saving and
overrides; failures retain the existing rollback. ByteFile adapters pass paths
to the existing executor. RegistryRecord<ListedPath> validates and serializes
without UTF-8 conversion. RPC/CLI production paths use ByteEvent, rendering valid
UTF-8 as JSON strings and opaque paths as arrays; CLI retry paths use write_all.
Legacy public String adapters remain, with checked errors for unrepresentable
paths, and are not used by production byte-event routing.

ExecutionPath::Bytes and String use equivalent path component validation and
converge on the existing execute_files workflow. No installer or authorization
guard has been replaced. Platform filesystem constraints remain: arbitrary
opaque names are not promised to work on Windows (existing WTF-8 boundary).
Raw URL byte transport remains out of scope and unsupported by this change.

Fresh test evidence is in `/workspace/shared/rebuild-pack-path-20261106`.
The original red.patch/red.log are from three complete-schema tests run on
clean a4ad20ac; all three fail with UnexpectedToken. The same tests pass after
implementation. Pure added unit tests cover executor-field adaptation and the
size-overflow guard, validator equivalence, JSON events and raw CLI event paths.
No actual installation, download, archive operation, authentication or launch
was executed. The legacy operational installation test receives only its
filename type conversion and is compiled, not executed. No full test suite or
ignored operational tests is invoked. Loader and override behavior are left to
separate work; same-file integration should retain both sets of changes.

The existing audit digest list contains modpack/rpc.rs but not most other changed
sources. Its digest is intentionally left unchanged pending independent review
and integration. Digest coverage is not claimed for unlisted source files.

## Fresh verification results

- New complete-schema integration tests: 3 passed after 3 baseline failures.
- New internal adapter/validator tests: 2 passed.
- RPC and CLI byte-event tests: 2 + 2 passed.
- Existing provider and schema unit tests: 5 + 10 passed.
- Five individually selected existing planning/schema integration tests: passed.
- Total: 29 Rust pure/in-memory tests; no tempfile-write tests selected.
- Existing-candidate proof guard controls: 6 passed.
- Locked all-target Clippy with `-D warnings`, fmt and diff checks: passed.

The first event-filter invocation accidentally selected zero tests; those lines
remain in green.log and are not counted. regression.log contains the corrected
filters and four executed event tests. No previous lost run is counted here.
Independent Zig oracle corpus execution is coordinated separately and is not
claimed as a completed check by this document.

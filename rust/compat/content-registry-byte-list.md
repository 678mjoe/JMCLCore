# Byte-preserving registered-content read/list view

This stage follows supplemental metadata bytes (`f5984b7`) and is strictly a
read/list change. Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

## Representation and unchanged write boundary

A content registry may contain a `file` byte array which is not UTF-8. Zig
validates it as bytes and lists the record even when no corresponding file exists
or the native OS cannot represent that path. The previous Rust String-only
registry decoder rejected the whole document.

`EntryRecord<F>` / `RegistryRecord<F>` share one schema, typed numeric decoder,
source validation, byte-based relative-path checks and duplicate checks. The
existing public `Entry` / `Registry` aliases remain **String-specialized**.
The listing view specializes only `file` to the existing byte-preserving
`ListedPath`. No placeholder strings, replacement characters or OS path
conversion are used to make records pass validation.

`Root::load`, `Root::save` and the legacy `Root::list` used by adoption still use
the String representation. The save signature does not accept a raw listing
registry, and this change adds no raw-view save function. The normal and listing
views share the same bounded registry-file reader and directory scan. Listing
compares known filenames by bytes, so registered raw names are not accidentally
reported as unmanaged. **Listing never opens a registered entry's path.**

RPC and JSON CLI output use strings for valid UTF-8 and byte arrays otherwise.
Text CLI writes filename bytes directly, for both client and Linux server lists.
All other registry fields, selector parameters, mutation/adoption functions,
artifact-store transactions, symlink/no-follow rules and hard-link ownership
remain unchanged. Arbitrary-byte registered paths still cannot be mutated or
adopted through the String view; this is a separate actionable compatibility
stage, not claimed complete by successful listing.

## Read-error fidelity

The negative fixtures exposed an existing datapack-list error mapping gap.
The original datapack RPC maps registry `UnsafePath` / `InvalidContentFile` to
`INVALID_PARAMS` with the selector message, and provider identifier failures to
its identifier message. Datapack text CLI has a specific invalid-CurseForge-ID
message. These corrections are guarded by **list only**; mutation mappings are
not broadened or altered. Plain UTF-8 controls distinguish this existing error
mapping from raw filename representation.

## Bounded executed evidence

Before the change, both new paired client/server tests fail on the raw registry.
The final local Linux suite passes five tests with no ignored tests:

- Four client families × nine success/error fixtures × RPC / text CLI / JSON CLI
  = **108 paired exact stdout/stderr/status comparisons**.
- Two Linux server families over the same matrix = **54 paired comparisons**.
- One configured registry-file alias is read without rewriting it or its target
  across all three surfaces; one physical raw registered-name fixture verifies
  suppression from the unmanaged list through an exact RPC pair.
- A direct read-view test proves that legacy String load/list still reject a raw
  registered path, compiles the unchanged save signature without calling it,
  and verifies no disk changes.

Mandatory record-only cases never create raw physical filenames: invalid UTF-8
`0xff` and surrogate-byte paths remain source data. They can therefore run on
all supported desktop targets, including filesystems that reject such names.
Success fixtures include ordinary UTF-8 and Unicode records; negatives cover
ASCII-array duplicate identity, traversal, wrong kind prefix, invalid hash,
length limits and provider errors. Snapshots preserve files, bytes, read-only
flags and file identities, including a shared immutable hard link. The explicit
registry-file alias target is also included in the snapshot.

The optional Unix physical-name case separately reports EILSEQ when the
filesystem cannot create that name. That is not a pass for physical raw-name
support and does not excuse the mandatory record-only cases. Server RPC tests
remain Linux-only; the Unix alias case is not claimed on Windows.

The prior unmanaged-listing tests and pure 189-case typed-string, 129-case
supplemental-byte, 84-case Modrinth, 115-case CurseForge and 204-case numeric
corpora are rerun. Existing content adoption/mutation test targets are compiled
with `--no-run`, **not executed locally in this stage**. Their files, adoption
implementation and mutation call sites are unchanged; established native CI
regression gates remain the integration owner's responsibility. Formatting and
all-target Clippy pass. Native acceptance still requires exact-head CI.

No adoption, remove/enable/disable/save RPC, archive/installer, network request,
Java, real account or game/server lifecycle operation is performed. Fixtures are
created directly in disposable test directories; the only invoked Core methods
and CLI commands are read/list operations.

```sh
JMCL_ZIG_ORACLE=/path/to/pinned-core \
  cargo test --locked --test content_registry_listing_bytes -- --include-ignored --nocapture
```

The static dispatch audit's pinned `content/mod.rs` and `content/registry.rs`
files must be reviewed and repinned on integration. Routing method guards and
family associations are unchanged; list representation and its exact errors
changed. No automatic digest refresh is appropriate.

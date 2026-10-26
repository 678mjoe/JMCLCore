# Registered byte filenames: local toggle and removal

RPC, client CLI and Linux server facade now operate on the byte-preserving
registry view for the existing `enable`, `disable` and `remove` operations.
Selections still use the existing String parameters. In particular an ASCII
project ID/slug can select a registered raw filename, and an ordinary file
selector works even when unrelated registry entries have raw filenames.

The public String-specialized Root methods are retained with their old parsing
behavior. Generic internal helpers preserve selector-before-registry validation,
provider selection asymmetry, physical rename/idempotence behavior, disabled
suffix handling and atomic registry save. Only internal byte-return methods are
used at protocol/CLI boundaries. Exact raw bytes are written to text CLI and
serialized as integer arrays in JSON; valid UTF-8 stays a string.

Content-relative `exists`/`unlink`/`rename` retain their pre-existing platform
symlink behavior. No CAS code, fallback policy, registry validators, parameter
decoders, archive logic or acquisition code changes in this commit. The source
oracle is Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, especially
`Service.setInstanceContentEnabled` and `Service.removeInstanceContent`.

## Evidence

Baseline failed the new disk-state assertion because Rust rejected a registry
containing unrelated raw records before it could disable the selected entry.
The final Linux corpus passes:

- New `content_mutate_bytes`: 3 tests, **981 exact paired invocations**: 972
  mutation steps plus 9 selector-before-corrupt-registry errors. Four client and
  two server families, RPC/text CLI, UTF-8/WTF-8/FF names, present/missing file/
  missing parent, repeated toggle and removal, enabled and disabled removal.
- Every step checks exact wire/text/error output and registry bytes against Zig.
  Unrelated raw entries survive; physical renames preserve the source/peer/CAS
  object's identity, bytes and readonly flags. Removal leaves the shared peer
  and object intact. Missing parents are never created by mutation.
- The String public API control rejects raw records without changing registry
  bytes or identity.
- Existing 7 content RPC oracle tests + 4 content CLI oracle tests pass, including
  configured aliases, selector/provider asymmetry, corruption and world locks.
- Prior byte adoption (2 tests / 73 local paired calls) and byte registry listing
  (5 tests, including 166 paired surfaces) remain green.
- Format, all-target Clippy and diff checks pass.

Windows physical WTF-8 behavior still requires native CI; it is not inferred from
Linux. Unix physical-name creation excludes only platform EILSEQ and reports it;
all other failures fail the test. All payloads are disposable opaque local files,
never parsed or launched. No network, Java or archive operations are invoked.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_mutate_bytes -- --include-ignored --nocapture
```

Set `JMCL_ZIG_SOURCE`/`JMCL_ZIG_ORACLE` to the pinned source and built binary.
Review and repin routing-source audit hashes during integration.

## Separate remaining representation work

- Dynamic RPC byte-array file selectors are not enabled by this change.
- Non-UTF-8 OS CLI arguments remain a separate process argument boundary.
- Remote/provider operational filenames and URL representations remain separate;
  this local mutation stage does not claim their acquisition flows are complete.

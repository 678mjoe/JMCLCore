# Opaque unmanaged filename listing

Content listing retains native Unix filename bytes rather than silently dropping
non-UTF-8 names. As in pinned Zig, a valid UTF-8 path is a JSON string; an invalid
UTF-8 path is a JSON byte array. Plain CLI output writes the path bytes unchanged.
Directory enumeration order and file-extension filtering remain unchanged. This
applies to client mods/resourcepacks/shaderpacks/datapacks and Linux server
mods/datapacks listings. The tests list ordinary opaque files; they do not parse
archives or adopt content.

`content_listing_bytes` compares raw RPC, JSON CLI, and text CLI output and checks
that file bytes, identities, and read-only attributes are unchanged. Unicode
controls run on every platform. Unix raw-byte creation is capability-tested:
only filesystem `EILSEQ` excludes that success fixture, visibly reported; other
errors fail. Windows unpaired-surrogate filenames are not covered by this slice.
The existing adoption API and registry paths still require UTF-8; raw-name
adoption compatibility is not claimed or changed here.

Reference: Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/service.zig::scanUnmanagedContent`, `src/protocol/content.zig`, and
`src/cli/content.zig`. No alternative ordering or lossy character replacement is
introduced. This fixes a Rust omission, not a compatibility exception.

Run the bounded corpus explicitly with:

```sh
JMCL_ZIG_ORACLE=/path/to/pinned/jmcl-core cargo test --locked \
  --test content_listing_bytes -- --include-ignored --nocapture
```

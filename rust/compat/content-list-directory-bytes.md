# LIST-only client content directory bytes

Only directory decoding for `mods.list`, `resourcepacks.list`, `shaderpacks.list`
and `datapacks.list` changes. A private directory variant retains byte arrays for
LIST using the existing dynamic-u8 decoder; all non-list directory parameters
still require Strings. World names remain Strings. Existing public Root String
methods remain, with additive crate-private native-path wrappers and the existing
checked `store::path_from_bytes` conversion. No lossy conversion or global CLI/
server/acquisition parameter change occurs.

The filesystem wrapper preserves Unix create_dir_all + RDONLY/DIRECTORY/NOFOLLOW/
CLOEXEC and Windows shared walk(..., true). World lookup still opens game/saves
before validating/opening the world, and keeps the existing mutable lock branch;
LIST passes mutable=false. No child registry/content/world directory is created.

## Exact baseline and source

Prepared source `209a4564df2754b549eac2120ba2a7b377d968ae` is the baseline.
The actual `content::handle("mods.list", ...)` unit with an ASCII directory array
was red (INVALID_PARAMS). Source provider protocol directory fields are byte
slices parsed from dynamic JSON, not typed metadata byte arrays. The pinned Zig
core is `5a4310debbcee26b3d2391bab2339f79cd6ec52e`:
`src/protocol/content.zig`, `src/protocol/datapacks.zig`, and service world lookup.
Source root opening uses createDirPathOpen (`src/install/installer.zig:283`).

## Bounded evidence

- 2 units: actual LIST acceptance and rejected non-list byte-directory controls
  (no mutation reached).
- 158 paired existing-root RPC cases: String/ASCII/Unicode/native opaque roots,
  dynamic integral/rounded/quoted numeric grammar, malformed elements, empty/NUL/
  blocked roots, datapack range/world/root ordering. Complete owned disk snapshots
  check contents/device/inode/mode/mtime/link count after every invocation.
- 15 separate root-creation cases allow only an empty owned root. Empty directory
  or empty datapack world range failures create nothing. Valid datapack world with
  missing saves creates only the game root then reports WORLD_NOT_FOUND.
- Native raw-FF directory creation is verified locally on Linux. The portable
  harness uses a WTF-8 lone-surrogate directory on Windows, and reports only
  explicit EINVAL/EILSEQ/ENOTSUP capability outcomes; other failures remain errors.
  This source portability is not a native Windows/macOS acceptance claim.
- Named prior client opaque listing and registry-byte listing tests pass.
  Formatting, all-target Clippy and diff checks pass locally.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_list_directory_bytes
python3 rust/compat/content_list_directory_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

The unique local target uses CARGO_PROFILE_DEV_DEBUG=0,
CARGO_PROFILE_TEST_DEBUG=0 and CARGO_INCREMENTAL=0 to bound storage.
Green executable SHA256: `877fa138c8eea91049f7c218d32a5057ccd11fea3ab628319c8430ad2396aefe`.
The change does not alter URL transport, adoption, installation, archive parsing
or Java behavior. Non-list byte-directory operations remain outside this slice.

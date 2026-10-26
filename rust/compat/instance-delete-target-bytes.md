# Instance deletion RPC byte targets

The delete-only handler now reuses existing `lookup_target`, dynamic-u8 ID and
directory decoding, checked native directory conversion, and `Root::open_native`.
Create, CLI argument handling and unrelated RPC operations are unchanged. The
old fallback's unreachable get subbranch was replaced only to give deletion its
own typed parameter and error path. No filesystem primitive or symlink policy
changes; deletion still validates the byte display-name view before the same
anchored `Root::delete`.

## Error ordering from the source

Pinned core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/protocol/instance.zig:284-313`, specifies:

1. Missing/invalid parameter shape and dynamic byte element errors.
2. Empty directory/ID range error, before opening the root.
3. Root open/native conversion error, before ID validation.
4. `FileNotFound` or `InvalidInstanceId` from deletion -> INSTANCE_NOT_FOUND;
   all other errors -> INSTANCE_DELETE_FAILED.

The fourth rule includes an invalid **stored** manifest ID, not just an invalid
request ID. A direct source/oracle control disproved the earlier assumption that
all malformed stored IDs must yield DELETE_FAILED. A valid but mismatching stored
ID still yields DELETE_FAILED, as do other malformed metadata. Both cases retain
the entire instance subtree. No blanket error normalization is introduced.

## Evidence and invariants

Baseline central tree d6ec6876 (binary SHA256
4a4b11a7098565583674743531a1d57d3fb9b30d6ccf31921448f80b80eb086b) had 46
mismatches in 48 paired RPC fixtures. The actual Rust delete-array test was red.
The expanded test invokes complete RPC executables, not a manual parameter
reproduction: this preserves outer null/duplicate request parsing semantics.

The 104-case matrix includes String/ASCII-array targets; FF/invalid/empty IDs; invalid and
missing roots; malformed numeric/nested elements; dynamic raw fractional versus
quoted fractional controls; absent/object/duplicate/ignored parameters;
requested versus stored ID errors; missing manifests and byte display names.
Owned missing-root creation effects are asserted, including creation before
nonempty invalid-ID errors and no creation for empty range failures.

Each case has only owned opaque files, a sibling sentinel and a hardlink peer
outside the target subtree. Failed deletion preserves the complete snapshot;
peer/sibling bytes and mtime survive all cases, with Unix identity/mode/link-count
checks. No symlink escapes, server lifecycle, artifact installation, Java,
archives or network calls occur. The explicitly scoped existing read-only
instance/server get/list/show scripts pass unchanged: lookup IDs (178 cases),
directories (128), manifest names (458). Both new tests, the prior delete-name
52-case corpus, three lookup unit tests, formatting, all-target Clippy and diff
checks pass locally.

```sh
cargo test --manifest-path rust/Cargo.toml --test instance_delete_target_bytes -- --include-ignored
```

Set `JMCL_ZIG_ORACLE` to the pinned executable. Byte directory conversion reuses
the current checked platform bridge; no lossy conversion or raw OS CLI support
is claimed. Native Windows acceptance remains separately gated.

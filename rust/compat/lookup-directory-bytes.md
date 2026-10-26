# Native directory bytes for read-only lookup and listing

Pinned instance/server get and list parameter schemas use `[]const u8` for
`directory`. ASCII arrays, valid UTF-8 arrays and (where supported by the host)
opaque native directory bytes must address the same configured roots as ordinary
strings. Rejecting all byte arrays or lossily replacing them with Unicode changes
which existing records can be read.

The get/list-only directory view uses the same **dynamic** u8 grammar as lookup
IDs, but does not apply ID validation to paths. It delegates to the existing
`store::path_from_bytes`: Unix preserves bytes; Windows performs checked WTF-8
to UTF-16 conversion, including unpaired surrogates. New `&OsStr` root-opening
entrypoints retain the existing mkdir/open sequence and no-follow final-root
flags. Existing `&str` entrypoints delegate without changing operational or CLI
parameter decoders. No acquisition, archive, lifecycle or Java operation is added.

```sh
python rust/compat/lookup_directory_bytes.py --zig "$JMCL_ZIG_ORACLE" \
  --rust rust/target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
cargo test --manifest-path rust/Cargo.toml --lib lookup_directory_tests
```

128 exact Linux comparisons pass with positive record assertions. Fixtures cover
ASCII/Unicode strings and arrays, opaque and malformed UTF-8 byte names, dynamic
versus quoted numeric precision, object-only params, malformed elements, empty paths, NUL rejection,
existing final files, root-before-ID semantics, final-root aliases, permitted
ancestor aliases, and source-defined missing-root creation. Complete snapshots
preserve preexisting bytes, inodes, modes, mtimes and hardlink counts. Missing-root
cases separately assert precisely the expected empty directory and preserved
siblings; they are not described as globally side-effect-free.

The portable corpus probes actual filesystem support. A physical raw-name
`EILSEQ` outcome does **not** count as successful raw-path coverage: it is printed
and both Core binaries must instead return the same root error without changing
disk. Windows malformed WTF-8 similarly requires deterministic rejection; a
valid unpaired-surrogate fixture uses a native wide path. Normal Unicode controls
remain mandatory. Windows aliases use owned directory junctions (no assumption
of symlink privilege). Native Windows/macOS execution remains pending CI; server
methods on non-Linux must retain their existing unsupported response, not be
counted as server filesystem acceptance.

This is only the get/list request-directory boundary. Other operation parameter
byte fields and persisted version/loader/Java-path representations remain separate
work. Existing global malformed-JSON first-error differences are unchanged.

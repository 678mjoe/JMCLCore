# Instance creation metadata validation

`instance.create` defaults to validation, as does `instance create` without
`--no-validate`. After request/argument validation and loader-conflict checks,
resolve the selected Vanilla, Fabric, NeoForge or Forge metadata through the
existing version service. BMCLAPI falls back to official metadata, while the
manifest retains the caller's requested source. No game, processor or installer
is executed: installer ZIPs are metadata inputs only.

Resolution finishes **before opening or creating the instances root**. RPC maps
metadata timeout separately to `METADATA_TIMEOUT`; other resolution errors map
to `VERSION_RESOLUTION_FAILED`. CLI prints the underlying Zig-compatible error
name. An invalid version must take precedence over a blocked root or invalid
manifest name. `validate: false` / `--no-validate` skips the resolver entirely.

The cache fixture corpus in `instance_validation_check.py` covers 71 exact
stdout/stderr/exit-status/on-disk outcomes against the pinned Zig executable:

- Default and explicit true validation; both sources; Vanilla, Fabric, NeoForge,
  modern Forge, processorless Forge and legacy Forge.
- Cached missing versions, invalid request fields, loader conflicts, blocked
  root error ordering and no instance-root write on resolution failure.
- Unknown uncached versions/loaders with validation disabled.
- BMCLAPI manifest misses falling back to the cached official metadata.

Run with absolute executable paths:

```sh
python3 rust/compat/instance_validation_check.py --zig /path/to/pinned/jmcl-core \
  --rust /path/to/jmcl-core-rust
```

Inputs are disposable synthetic old-format metadata cache entries; no real
network/account/game is required. Fixtures mirror the established
`version_check.py` corpus. Linux execution passed; this change does not claim a
new native Windows/macOS validation run. The resolver's real transport behavior
remains covered by the metadata subsystem, not by these cache-only tests.

The blocked-root case also fixes an existing Unix diagnostic discrepancy:
`create_dir_all` reports `AlreadyExists` for a final regular file; Zig's
`createDirPathOpen` proceeds to the directory open and reports `NotDir`. Rust now
lets the existing nofollow directory open produce that error. Trusted-root and
manifest read-alias policies are unchanged.

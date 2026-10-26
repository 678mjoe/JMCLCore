# Linux local server CLI native roots

The existing OS-argument hook now accepts native `--servers-directory` only for
`server mods|datapacks list|enable|disable|remove` on Linux. Existing mutation
`--file` handling remains supported. ID, project and option names stay text-only;
other routes/fields, adoption, provider operations and hidden lifecycle dispatch
are not broadened. main.rs remains byte-identical.

A private local parameter struct carries the real directory bytes, not a lossy
String or fake Params.directory. It preserves option parsing, last duplicate
selection, LIST --json, empty values and consumed-option errors. After ID/options
and exactly-one-selector validation, native root/manifest, mutation-only guard,
properties/world and selector handling retain source order. The real read-target
helper is shared; the existing final output renderer is extracted byte-for-byte.
Operational Params and original String CLI parser remain unchanged.

Pinned source: 5a4310debbcee26b3d2391bab2339f79cd6ec52e.
Base: fc50f0f29d6687010475f16dce675891a7288c87, binary
8cf72f46ed476bccb4f6783318e3534ca6e4242a1e1d4d50a0297cbc4270850f.
Baseline: 800 paired sequences / 1,280 CLI pairs; every one of the 400 opaque-root
sequences panicked in Rust's fallback UTF8 argv conversion, while all ASCII and
Unicode controls matched source. The focused new hook unit was also red.

After the fix, all 1,280 pairs match exactly with zero invariant failures. Cases
cover FF and WTF8-encoded raw bytes on the Linux filesystem, ordinary Unicode,
JSON/text LIST, mutation/repeat, ID/manifest/root/guard/property/selector errors,
unknown/missing/consumed options and raw/text/empty duplicate roots. Physical
root creation rejects only explicitly recorded EILSEQ capability cases; other
errors fail. These Unix byte fixtures do not prove Windows native filename use.

All fixtures are owned, server.pid is absent before/after, guards are precreated
unheld or malformed leaves, and no live process matching occurs. Every failure
and LIST keeps a full raw snapshot unchanged; mutations check exact registry
bytes, paths and immutable readonly peer identity/content. argv bytes and raw
outputs/status are preserved. No CAS/adopt/install/network/Java/archive call runs.

Prior local CLI 1,232 pairs plus 80 argument controls and the three existing
native-argument tests pass (including raw --file cases). The former negative
server-root hook control is replaced by positive root coverage and an unchanged
raw-project rejection control, not silently removed. Clippy/fmt/diff checks pass.

```
cargo test --test server_cli_native_root
python rust/compat/server_cli_native_root.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

At this stage, non-Linux tests assert ordinary Unicode platform rejection only.
The raw-server fallback panic there remains a separate source-proven gate issue:
Zig rejects unsupported platforms before parsing local arguments. This Linux fix
must not be described as native server support or a global raw-argument repair.

The separate `server-cli-platform-gate.md` follow-up implements the recognized
non-Linux local-route gate; native execution remains explicitly pending there.

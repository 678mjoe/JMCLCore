# Local mutation directory byte strings

The private local content RPC parameter parser now decodes `directory` with the
same dynamic byte grammar for list, enable, disable and remove. The String-only
mutation restriction was not present in pinned Zig
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` (`src/protocol/content.zig`,
`ContentRemoveParams` and toggle/remove handlers; `src/protocol/datapacks.zig`,
`DatapackSelectorParams`). Public Root String entrypoints are unchanged.

Range/exactly-one-selector validation still precedes root acquisition; mutable
world selection and its lock guard still precede selector validation and registry
mutation. Native path conversion uses the already reviewed capability-relative
root bridge with unchanged create/no-follow flags. `world` remains String.
Adoption dispatch returns before this parser; acquisition methods are not routed
through it. No adoption/acquisition execution is needed or included.

Validation:

```
cargo test --test content_list_directory_bytes
python rust/compat/content_mutation_directory_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

The former non-list rejection test now asserts `CONTENT_NOT_FOUND` on an existing
empty root and no created files, instead of dropping coverage. It fails on the
prior tree with `INVALID_PARAMS`. The Linux executable oracle harness runs 270
fresh paired fixture sequences (540 RPC pairs), with positive expected source
outcomes as well as exact comparisons. The prior tree differed in 147 sequences.
All content is opaque synthetic bytes; jar/zip suffixes satisfy registry rules,
not archive parsing. No network, CAS calls, installers, Java or lifecycle actions.

Controls cover ordinary Strings, ASCII and opaque native arrays, integral versus
fractional dynamic numbers and quoted numbers, blocked/empty roots, selector
range and validation, missing physical files, missing/invalid worlds and an
exclusively locked **owned** world lock. Direct assertions cover registry state,
physical paths, repeat toggle `changed=false`, repeat removal `CONTENT_NOT_FOUND`,
and immutable peer inode/mode/mtime/content (expected link count changes only).
Failed operations preserve full snapshots; sibling data remains unchanged.

Ordinary String/ASCII-array and error cases run on every native OS. Only held
`flock` controls are Unix-specific. The opaque directory probe uses raw FF on
Unix and WTF-8 lone-surrogate paths on Windows; only exact EILSEQ (Unix) or
ERROR_INVALID_NAME (Windows) suppress that physical capability subset, and the
probe result is printed. Other failures remain strict. Both engines are captured
before invariant failures are reported; equal readonly failures do not count as
a positive pass. Windows fixtures are retained and their owned directory printed
rather than clearing shared readonly attributes during cleanup. Local validation
was Linux only; this is native-test preparation, not Windows/macOS acceptance.
The independent eager-Value/unknown-large-number issue remains unchanged, as do
raw world parameters, server mutation targets and wider CLI argument boundaries.

Default invocation prints every mismatching named row as JSON, including both
responses and normalized registry/path/link states. Separate per-engine
`diagnostics` retain actual peer before/after device, inode, mode, mtime, link
count and bytes; these identities are deliberately not compared across separate
fixtures. Invariant failures also include the complete row. A red baseline
rerun printed all 147 mismatches rather than truncating the first 12.

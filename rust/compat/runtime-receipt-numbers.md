# Persisted runtime receipt numeric fidelity

This slice reads synthetic `.jmcl-runtime.json` records and lists their metadata;
it does not install, acquire, extract, probe, select, execute or remove a runtime.
The receipt's executable path is deliberately nonexistent in the new corpus.

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/java/java_runtime_store.zig::loadReceipt`, uses a typed JSON schema. A raw
`schema_version: 1.0000000000000001` is invalid and omitted from listings. Rust
previously rounded it to 1 through binary64 and listed the record. Conversely,
legitimate typed string grammar such as `"1.0"` was incorrectly rejected.

Persisted receipt fields now retain raw lexemes and use typed binary128 numeric
coercion with their declared u16/u32/u64/i64 bounds. Signed minimum and exactly
one optional sign are handled explicitly. Record validation and listing's
invalid-record skip policy are unchanged. The pre-existing `de_u64` helper used
by `runtime_plan.rs` remains byte-identical; this change does not alter that
planner or any acquisition path.

`java_receipt_numbers` covers 175 raw-token records across schema, major version,
file count, byte count and signed installation timestamp. Each record has exact
RPC, text CLI and JSON CLI comparisons (525 paired results), with receipt bytes,
hardlink identities and directory contents preserved even when invalid. Separate
positive/negative parser expectations cover precision and signed boundaries.
No executable is present, so these tests cannot validate a real Java runtime or
claim automatic runtime installation works.

```sh
JMCL_ZIG_ORACLE=/path/to/pinned/jmcl-core cargo test --locked \
  --test java_receipt_numbers -- --include-ignored --nocapture
```

# Ordered persisted content registry parsing

The private persisted-wire parser follows the pinned
`src/content/content_registry.zig` schema in encounter order. The public
`Kind`, `Provider`, `Source`, and `RegistryRecord` serde contracts remain
unchanged; load/save and operation consumers are not rewritten.

The prior actual `registry::parse` implementation displaced an earlier invalid
unknown string with a later type, duplicate, missing-field or EOF error. The
retained 23-case corpus demonstrates those differences, including malformed
escapes in otherwise complete documents. Fully typed unknown raw-UTF8 and
lone-surrogate acceptance had already been rejected; this change is not a
claim that those documents were previously accepted.

`registry_ordered.rs` constructs the existing wire structs with the audited
private cursor. Known duplicate fields are detected before their replacement
value, unknown values are lexically consumed in encounter order, and required
fields are checked at the end of each object. Unknown nesting uses the cursor's
iterative input-bounded stack, not recursive descent or an invented depth cap.
Numeric source spans retain binary128 typed coercion; enum spellings keep their
separate integer-only tag grammar. Null optional fields still count as seen.
Defaults remain schema 1, empty entries, enabled true, dependency false, and
null optional source fields.

The entire typed document and trailing-input check finish before unchanged
schema/count/entry validation. The existing 1,024-entry and 4 GiB entry-size
limits and sequential path/hash/source/duplicate validation remain unchanged.
The Rust parser retains its existing 256 KiB input guard. This is the original
**load-facing file limit**, not a claim that the pure Zig `parse` helper imposes
an identical total-document bound: that helper sets a maximum token value
length. The boundary test therefore asserts Rust's preserved input guard
separately from source-equal pure buffers.

Private file conversion preserves the two existing views. String parsing still
rejects a decoded non-UTF8 file at that field; byte-view parsing retains the
bytes and reaches path validation. The raw `[255]` filename example is not
counted as String-parser parity: String returns `UnexpectedToken`, while the
byte view reaches `ContentKindPathMismatch`. A valid `mods/` byte path is retained
by the byte view. No lossy conversion or mutation/API broadening is introduced.

## Bounded evidence

* Actual earlier-lexical regression failed before implementation.
* 359 exact pinned pure registry comparisons cover the original 23 differences,
  inverse error order, defaults, typed fields and numeric boundaries, nullable
  duplicates, malformed closing delimiters, ignored large numbers, depth 160/512,
  and complete typing before entry-count/semantic checks.
* A reviewer found a wrong-close classification in the draft entries-array
  parser. The pinned oracle reproduced it; the retained regression now passes
  using struct-child token consumption.
* Separate pure tests preserve the 256 KiB input boundary and String/byte view
  conversion behavior. Existing 99 enum, 212 source-byte, and 189 typed-array
  corpora remain required regressions.

```sh
JMCL_ZIG_SOURCE=/path/to/pinned/core \
cargo test --locked --manifest-path rust/Cargo.toml \
  --test content_registry_ordered -- --include-ignored --nocapture
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::registry::ordered::tests::registry_file_views_preserve_conversion_boundary \
  -- --exact
```

The source adapter performs parsing only. Its owned input bytes, mtime,
readonly status and Unix device/inode identity are checked unchanged. These
checks do not invoke load/save, adoption, mutation RPCs, artifact installation,
network operations or Java. Linux evidence does not substitute for native CI
or claim exhaustive malformed-JSON equivalence.

# Dynamic byte numbers in remote content parameters

The original content/datapack install and version-change handlers decode their
parameter structs using `std.json.parseFromValue`, not direct typed JSON parsing.
Consequently numeric byte-array elements first pass through dynamic JSON Value:
finite raw JSON floats use binary64, whereas quoted numeric spellings retain
the typed conversion path. The target element type is u8.

The actual Rust private `content::rpc::parse` helper used `Json::u64` for string
array elements, applying typed binary128 conversion before a u8 range check.
A pure 48-case proof found four acceptance differences: raw
`[109.00000000000000001]` must decode to `"m"` in each of the four parameter
schemas. Rust rejected it. The corresponding quoted fraction must still fail.

Only the element conversion in `content::rpc::text` changes, using the existing
`json_number::dynamic_raw` helper at width 8. The zeroizing vector and decoded
string, UTF-8 restriction, optional/default fields, parameter evaluation order,
and handler bodies are unchanged. Workers/retries already used the dynamic
helper. No public metadata schema, auth parser, global number semantics, URL,
transport or artifact behavior is modified.

## Pure evidence and limits

Three dedicated tests call only `text` and the actual private parameter parser:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::rpc::params_number_tests::remote_params_byte_numbers_follow_dynamic_not_typed_rounding \
  -- --exact
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::rpc::params_number_tests::remote_params_text_utf8_boundary_is_unchanged \
  -- --exact
JMCL_ZIG_SOURCE=/path/to/pinned/core \
cargo test --locked --manifest-path rust/Cargo.toml --lib \
  content::rpc::params_number_tests::remote_params_buffer_values_match_pinned_dynamic_schemas \
  -- --exact --ignored --nocapture
```

The source test verifies all four copied private Params declarations against
pinned source and verifies their `ignore_unknown_fields=true` parse options.
It invokes only standard JSON Value-to-schema parsing. The source checkout must
be clean at `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, using Zig 0.16.0;
text-source checks account for native CRLF checkout conversion.

The 980-case finite matrix covers nine text fields across the four schemas,
raw-versus-quoted precision, integral/fractional/overflow values, numeric spelling,
valid multi-byte UTF-8, optional nulls, missing fields, unknown fields, defaults,
and field-order controls. Successfully decoded values must match exactly.
Both rejected means **acceptance agreement, not literal error-name parity**:
the Rust helper intentionally returns `()`, while the source has detailed
underlying parser errors. Failure diagnostics retain both outcomes. Explicit negative controls reject
missing, extra, empty or wrongly typed error fields so arbitrary oracle objects
cannot pass as rejections.

Direct controls preserve the existing non-UTF8 rejection: `[255]` remains
unsupported by this String parameter decoder, while `[195,191]` is valid UTF-8.
The test does not silently widen opaque paths or claim complete request-envelope,
semantic-validation, or wire error-message coverage.

Inputs use disposable files only for the pure adapter and remain unchanged in
bytes, mtime, readonly state and Unix device/inode identity. Synthetic API-key
field values test decoding only; no credentials are used. No content handler,
resolver, installation, transport, filesystem-root operation, Java, archive,
network request or authentication operation is invoked. Native CI remains
separate from local Linux evidence.

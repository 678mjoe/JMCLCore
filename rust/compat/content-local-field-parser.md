# Ignore unrelated local content numbers without eager coercion

Pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e` parses local content
parameter structs with `ignore_unknown_fields=true` (protocol/content.zig and
protocol/datapacks.zig). An unrelated `1e400`, including nested values, does not
require conversion to finite binary64. Rust previously materialized every field
as serde_json::Value before constructing its existing borrowed field map.

The private local parser now validates complete JSON strings through the existing
borrowed lexical validator, constructs the same RawValue field map, and decodes
only the required world String. Known directory/project/file fields retain their
existing dynamic byte/numeric decoder; optional/null selectors, ignored LIST
selectors, exactly-one-selector/root/lock/semantic ordering are unchanged.
World byte support is not added. Adoption and acquisition dispatch are untouched.

Actual RPC still rejects malformed JSON/Unicode and duplicate keys before local
dispatch. Direct-helper lexical rejection is retained too; direct-helper last-key
behavior is intentionally unchanged rather than mistaken for RPC duplicate policy.

Validation:

```
cargo test --lib content::tests::local_field_parser_ignores_unrelated_numbers_but_validates_strings -- --exact
cargo test --lib content::tests::local_project_byte_parse_and_semantic_boundary -- --exact
python rust/compat/content_local_field_parser.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

The new helper test is red on the prior tree. The executable corpus covers 1,064
paired requests: four local content families, LIST and absent-selector
mutation requests, unknown/known fields, huge/ordinary/underflow numeric values,
null/bool/String, malformed surrogates/JSON, duplicate nested and top-level keys,
and missing/non-object parameters. Complete owned disk snapshots must remain
unchanged; no registered artifact, acquisition, installer, Java or archive calls.
The existing 540-pair owned mutation corpus also guards successful operations.

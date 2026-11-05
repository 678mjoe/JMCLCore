# Separate typed modpack numeric tokens from dynamic RPC values

Local continuation from `8cdf52d`; not included in published `190be14`.

Modpack schema and provider document numbers were deserialized through Value and
lost source precision. Zig parses these documents directly from source bytes,
using binary128 for float-shaped integer tokens. In contrast, modpack RPC uses
parseFromValue and must retain dynamic numeric Value rounding.

The fix adds a RawValue-based typed helper, switches schema/provider u32/u64
fields, and uses a separate typed u8 helper for provider hash algorithms,
dependency kinds and release types. RPC workers/retries retain the existing
dynamic u8 helper. String Values retain typed grammar in both paths.

Pure regression scope:
- u32 fractional token rejection and binary128 rounding, exact wide u64 values
  and the u64 upper boundary.
- RawValue preservation through Object, optional_object and object_vec.
- Provider Envelope/Object/Hash and optional Pagination wrappers.
- Dynamic numeric token `3.0000000000000001` still yields 3, whereas that string
  is rejected.
- Fixed Zig 0.16.0 parseFromSlice checks on u8, u32 and u64 establish the reference
  behavior without invoking the Core.

The new Rust precision regression failed before the fix. The independent source
review confirms no HTTP client, RPC handler, archive or installation execution is
introduced by these tests. No full real-document differential, provider response,
archive read or installation claim is made.

Validation completed: all three scoped Rust tests pass, as does the pinned Zig
pure-token test. Formatting, all-target Clippy and 22 guard tests in normal and
optimized Python modes pass. No audited source fingerprint changed in this
batch; the static report still records zero Core requests and no replacement.

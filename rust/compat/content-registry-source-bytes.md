# Registry source/hash byte fields and validation order

The pinned `5a4310de` `src/content/content_registry.zig` first deserializes the
**entire typed registry**, then validates schema/count, file, SHA-1, size,
provider source, and duplicate entries in that order. SHA-1 and optional source
IDs/slug are typed byte slices; non-UTF-8 byte arrays are legal JSON input but
fail the later ASCII/provider validation. Rust previously converted them to
`String` during deserialization, returning `UnexpectedToken` too early.

A private, deserialize-only wire view now retains those bytes. Shared borrowed
byte views keep public `RegistryRecord::validate` and parsed-wire validation on
the same ordered rules. Only after validation are guaranteed-ASCII values
converted to the existing public String-backed `Source`/entry fields. The public
Deserialize derives, mutation/storage aliases, serializer, save API and file
representation parameter are unchanged. Repository source inventory found
`parse_as` to be the production direct registry deserialization site; this change
does not promise semantic validation through arbitrary external direct serde
calls. The String file view retains its existing raw-filename limitation; the
separate listing view keeps its existing byte-capable behavior.

Completed typed parsing also checks JSON strings in ignored fields before
semantic validation. Invalid raw UTF-8 and lone surrogate escapes are rejected
as `SyntaxError`, while ordinary unknown fields, huge ignored number lexemes,
valid surrogate pairs and escaped literal backslashes remain supported. This is
not an eager precheck: earlier typed/duplicate errors remain earlier. An invalid
unknown string *before* a later typed error still has a known diagnostic-order
gap; this patch does not claim a streaming lexer redesign or complete malformed
JSON diagnostic parity.

`content_registry_source_bytes.rs` contains 212 pure pinned comparisons including
all 43 initial proof cases. It covers source fields across all three providers,
SHA-1, FF/WTF-8 arrays, normal strings/ASCII arrays, typed-u8 failures, ID bounds,
schema/path/hash/size/provider order, later-entry enum failures, duplicate files
and projects, and completed-parse lexical acceptance. Inputs' bytes, mtime and
permissions are preserved (Unix identity is checked too). The listing/public
validator unit test separately checks the shared byte rules. No load/save,
mutation RPC, adoption, CAS, provider transport, archive or JVM operation runs.

```sh
JMCL_ZIG_SOURCE=/path/to/unchanged/pinned/source \
  cargo test --locked --test content_registry_source_bytes -- --include-ignored --nocapture
cargo test --locked --lib content::registry::wire_tests
```

The fixture compiles the existing pure registry adapter using Zig 0.16.0 and
verifies the source revision. Two direct regressions fail against the prior
Rust library and pass with the wire-view fix. Native Windows/macOS execution
remains a CI gate, not a claim inferred from local Linux results.

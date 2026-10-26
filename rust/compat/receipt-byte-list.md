# Persisted receipt byte fields in metadata listings

Pinned source: Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
`src/java/java_runtime_store.zig` (`RuntimeReceipt`, `loadReceipt`, `listRuntimes`).
Its typed byte slices accept JSON strings or arrays of typed u8 values, and its
provider enum accepts names and integer-shaped ordinal tags. The previous Rust
listing silently omitted otherwise valid records with array-form fields or
ordinal providers. A baseline run of the new corpus fails its positive
`id-ascii-array` assertion: the original returns a record and Rust returns none.

A private listing-only `ReadReceipt` retains the ten byte-slice fields. UTF-8
bytes serialize as JSON strings, opaque bytes as arrays; text output writes
version and executable bytes directly. Null versus empty optional metadata,
control bytes, typed numeric precision, enum grammar and validation order remain
source-compatible. These are metadata values, not paths opened or executed by
this view. In particular, opaque executable/home bytes undergo the source's
relative-path syntax validation without resolving a runtime executable.

Ignored nested strings still require well-formed JSON: invalid UTF-8 and lone
surrogate escapes are rejected, while paired surrogates, literal escaped
backslashes and ignored `1e400` remain accepted. This validation is private to
the listing parser; it does not decode unknown fields into generic values.

The public operational `Receipt`, `Provider` deserializer and `receipt::parse`
are unchanged. `store::list` still serves managed selection unchanged.
`list_read` is a separate anchored scan used only by runtime-list RPC and CLI;
its root handling, enumeration, name/count checks, read limits, alias handling,
cleanup loop and sorting match the existing scan. This intentionally does not
claim that managed selection now accepts all listing representations. Shared
planning, acquisition and writing code is unchanged.

## Bounded evidence

`receipt_byte_list.py` checks 103 receipt cases and, on Linux, 324 exact paired
RPC/JSON-CLI/text-CLI results. Positive cardinality assertions prevent equal
failures from passing as successful listings. Cases cover all ten ASCII array
fields, opaque/control metadata, provider names/ordinals and rejected forms,
typed-u8 grammar and fractional rejection, field/record limits, malformed and
duplicate fields, ignored large numeric tokens, directory-name sorting, and
256/257 valid-record count boundaries. The count failure has independent error
and status assertions. Existing receipt and sibling bytes, modes, mtime, inode
and hardlink count are checked after every call. Preexisting roots are preserved;
a missing-root case explicitly asserts the normal root/runtimes creation effect.
The corpus therefore does not claim that listing is universally side-effect free.

Unix fixtures additionally check a receipt-file alias and a skipped directory
alias. Windows does not assume symlink privileges and explicitly reports that
fixture exclusion; ordinary hardlink identity remains checked. No fixture
contains a staging directory or executable file. Only `java.runtime.list` and
`java runtime list` are called: no probe, selection, removal or installation.
Three pure unit tests check serialization, typed grammar, operational-parser
isolation and ordered validation. The existing list-only numeric corpus remains
525 exact paired results over 175 receipts. Native macOS/Windows execution of
this new slice remains a separate CI gate, not a Linux result.

```sh
python rust/compat/receipt_byte_list.py \
  --zig /path/to/pinned/jmcl-core --rust rust/target/debug/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
cargo test --locked --lib java::receipt::read::tests
JMCL_ZIG_ORACLE=/path/to/pinned/jmcl-core cargo test --locked \
  --test java_receipt_numbers -- --include-ignored --nocapture
```

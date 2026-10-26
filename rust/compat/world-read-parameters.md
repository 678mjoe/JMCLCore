# Read-only world RPC byte parameters

This slice owns only `worlds.list` and `worlds.get`. Pinned Zig
`src/protocol/worlds.zig` declares directory/world/optional Minecraft version
as dynamic byte strings; ordinary Strings and numeric byte arrays are both
accepted. The previous Rust shared Value decoder accepted only Strings,
rejecting valid lookups before filesystem and semantic validation.

A dedicated read-only parameter view now uses the existing dynamic-u8 grammar
(raw numeric binary64 versus quoted typed-number parsing), plus existing
whole-input lexical validation. Unknown numeric fields need not be converted
to finite floats. The outer protocol retains duplicate and malformed Unicode
handling. Mutation/archive decoding remains unchanged.

The read helpers preserve empty-field checks before filesystem access, then
root/saves acquisition before world-name validation. Native name lookup is
an additive overload using exactly the existing Unix follow flags or Windows
handle-relative follow helper. Byte versions compare directly to the metadata
version bytes: opaque nonempty input is different, not a lossy replacement or
parameter error. Public operational String APIs are unchanged.

The original request-byte slice kept physical enumeration separate. The
subsequent [read-view enumeration correction](world-list-names.md) preserves
opaque names and source count/order semantics for this RPC view. String CLI
and raw temporary-name cleanup remain separate boundaries. Existing metadata
field representation is unchanged.

## Bounded executable corpus

```sh
python rust/compat/world_read_parameters.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/checkout-at-5a4310d
cargo test --locked --lib worlds::read::tests
```

Fixtures are owned preexisting roots, empty saves or an Alpha world with
minimal synthetic level.dat metadata and inert icon bytes. Metadata decoding
is read-only; no archive import/export/backup operation is invoked. There
are no temporary/staging cleanup candidate names: the unchanged list cleanup
traversal has nothing to remove. Complete before/after bytes, modes, mtimes,
inodes and hardlink counts must remain identical for each request.

Cases cover positive String/array empty-list and get results, dynamic/quoted
byte grammar, required/empty fields, malformed elements, root/saves/name
precedence, invalid UTF-8 version comparison, ignored huge numbers, outer
Unicode/duplicate controls, native directory/world targets and owned aliases.
Positive assertions require actual metadata/name/version relation, never
merely matching errors. Physical raw-name support is probed; only filesystem
EILSEQ or Windows ERROR_INVALID_NAME permits rejection-mode testing, which
still compares exact responses and preservation. Native platform verification
is separate from Linux evidence. No servers, process fixtures, Java,
acquisition, network or mutation methods are involved.

The baseline source-only diagnosis was reproduced by 88 actual read requests:
42 wire differences, with all disks unchanged. The checked-in expanded corpus
also fails on that baseline's array-directory positive list case. Final Linux patched execution passes **128 exact request comparisons**,
three focused parser/name units and the existing named icon/lock/world alias
read-only regression, with library Clippy warnings denied. Windows-only
malformed-WTF8 directory controls require the source root-error class, not a
parameter error; native macOS/Windows execution remains pending.

### Object-shape regression control

The initial read-view implementation accidentally allowed serde's positional
sequence representation for derived structs. Actual list `[directory,null]`
and get `[directory,world,null]` calls succeeded where Zig requires an object.
An explicit object-shape check now follows lexical validation and precedes
field deserialization. Sixteen raw RPC array/scalar/null controls and a focused
unit prevent this regression; valid objects and ignored large-number behavior
remain covered. The corrected Linux corpus has 144 exact comparisons.

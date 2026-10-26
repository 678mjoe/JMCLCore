# Public-account parameter byte arrays

Zig's `src/protocol/auth.zig` parses all four account metadata handlers through
`std.json.parseFromValue`. Byte slices accept strings and arrays; unquoted array
numbers first pass through dynamic binary64. This differs from the directly
parsed persisted metadata's typed binary128 grammar.

## Boundary and order

Private `ParamBytes` and `ParamAccount` retain decoded bytes without changing the
public serialized `Account` string model. Only known fields are decoded;
unknown fields remain ignored by typed deserialization rather than materialized
into a generic JSON object. Malformed/overflow array elements fail parameter
parsing. An in-range byte such as 255 parses successfully even though it cannot
be valid account metadata. Rejecting it during UTF-8 deserialization would emit
the wrong public error.

After parsing, handler empty checks occur first. Root validation/opening then
precedes ID or account validation, matching `Service.saveAccount`, `loadAccount`
and `deleteAccount`. UTF-8 conversion occurs at this semantic boundary. Existing
ASCII/UUID/XUID validation remains authoritative. Therefore a blocked root and
an invalid-but-parsed ID return the root operation error; malformed ID elements
still fail before touching the root. An invalid account can create an empty new
valid root, as in Zig, but cannot write an account manifest.

## Bounded evidence

The preparatory read-only evidence comprised 38 get/list requests plus 48 pure
pinned-Zig parsing/validation cases. The committed regression
`compat/account_param_bytes.py` covers 115 paired RPC requests:

- Exact read-only get/list outputs and shared invalid get/delete target cases.
- Each of four account fields encoded as valid arrays, quoted bytes, rounded
  binary64 tokens, and malformed/overflow/non-ASCII controls.
- Invalid saves preserve the complete preexisting disk snapshot, including an
  unrelated account and an external hardlink.
- Successful replacement preserves the external hardlink's bytes, identity,
  mode and modification time while detaching the newly replaced manifest.
- Root-file, new-root and empty-root precedence; exact partial disk effects.
- A successful array-form save/get/list/delete sequence followed by not-found.

Every success case requires actual decoded metadata or deletion, not merely
matching errors. Pure units distinguish raw binary64 rounding from quoted/typed
binary128 behavior and retain non-UTF8 bytes until semantic validation. Complete
stdout and paired disk contents are compared to source revision
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`. Tests use only owned temporary public
metadata, never authentication, credentials, network or real accounts. Linux
local execution passes; added macOS/Windows execution remains a native CI gate.

```sh
python rust/compat/account_param_bytes.py \
  --zig /path/to/pinned/zig-core --rust /path/to/rust-core \
  --oracle-source /path/to/pinned/source
cd rust
cargo test --locked --lib accounts::
```

The disk-only parser and its [separate corpus](account-disk-bytes.md) retain their
typed grammar. This is an account metadata compatibility correction, not an
extension to authentication APIs or a general arbitrary-byte wire redesign.

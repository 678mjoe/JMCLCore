# Read-only lookup ID byte arrays

Pinned Zig `src/protocol/instance.zig` and `src/protocol/server.zig` parse lookup
parameters from dynamic `std.json.Value` into byte slices. An ASCII byte-array
ID can therefore address an ordinary existing instance/server. Raw floating
array elements have binary64 dynamic semantics; quoted numbers retain their
string grammar. This differs from the typed persisted display-name parser.

`instance.get` and Linux `server.get` now use a private lookup-only parameter
view. IDs remain bytes through empty checks and root opening. Only afterwards
are UTF-8 and the existing portable ASCII ID rules checked; invalid bytes
produce `*_NOT_FOUND`, never a lossy filesystem ID. Root failure still wins
over a nonempty invalid ID. Array-element/type failures remain `INVALID_PARAMS`
with the original parse diagnostic; an empty array uses the range diagnostic.
Operational create/delete/lifecycle decoders are unchanged.

The raw selected-field view also preserves ignored huge number tokens for
`server.get` (for example `unknown: 1e400`), which its previous intermediate
`serde_json::Value` rejected. Ordinary unrelated lifecycle fields are ignored
as in the original target struct. This is not a global decoder change.

```sh
python rust/compat/lookup_id_bytes.py --zig "$JMCL_ZIG_ORACLE" \
  --rust rust/target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
cargo test --manifest-path rust/Cargo.toml --lib lookup_id_tests
```

178 exact paired RPC observations pass on Linux, with positive record/status
assertions, byte/grammar errors, blocked-root precedence and unknown-field
controls. Forty duplicate-field/malformed-JSON cases verify the outer
`INVALID_REQUEST` gate in both orders before either lookup decoder; no new
duplicate policy is introduced. The corpus snapshots all preexisting owned files and unrelated
hardlinks. Separate disposable missing-root fixtures verify the original read
API's side effect: `openRoot` uses `createDirPathOpen`, so get creates an empty
root before returning not-found for a nonempty invalid/missing ID. Empty or
malformed IDs do not create that directory. There are no mutation, lifecycle,
installation, discovery or Java calls. Native other-platform execution remains
a CI gate; server unsupported-platform behavior must remain unchanged.

## Remaining separate boundaries

- Directory byte arrays/raw native paths are addressed by the separate checked
  [get/list native path bridge](lookup-directory-bytes.md), not UTF-8 rejection.
- The independently observed `server.list` ignored-huge-number issue is closed
  by its separate [directory-only read view](server-list-params.md).
- CLI show takes argv IDs, not JSON array parameters; its existing behavior is
  unchanged. Operational and other request string fields are outside this slice.

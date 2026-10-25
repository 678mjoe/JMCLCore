# Authentication port boundary

This module implements the five authentication RPC state machines against an
injected `Transport`. It does not persist credentials, create an account vault,
add a login CLI or add PKCE endpoints absent from the pinned Zig core. Its tests
use only synthetic scripted responses; the HTTPS adapter is implemented but is not live-service-tested.
The production `HttpsTransport` binds only the official endpoint/method pairs,
uses certificate-verified HTTPS, 30-second per-request total timeout, and bounded
64 KiB response reads. It never sends fixture requests during tests. The exported
`handle` is the direct typed writer binding for the shared protocol dispatcher.

Pinned `src/auth/http.zig` explicitly uses `.redirect_behavior = .unhandled`;
Rust uses `Policy::none`, not a new redirect-policy change. Both send
`Accept: application/json` and `Accept-Encoding: identity`; bearer bytes must be
printable ASCII. No cookies or persistent credential storage are introduced.

## Owned-secret handling

* `Secret` has no `Debug` or `Clone`; explicit serialization is the release point.
* Request and response storage is wiping-owned. `WipingBuffer` grows by allocating
  replacement storage, copying, and dropping/wiping the previous allocation. A
  normal `Vec::reserve` can leave an unwiped old allocation after reallocation.
* Parsing borrows raw JSON spans. The audited serde_json 1.0.154 `RawValue` path
  invokes `ignore_value` / `ignore_str`; scratch contains only container markers,
  not decoded values. `json.rs` manually decodes only requested strings into a
  preallocated `Zeroizing<String>`; errors drop/wipe that owned decoded storage.
  Re-audit that library path when upgrading serde_json.
* Successful terminal responses are typed and serialized directly to the supplied
  output writer; tokens are never cloned into `serde_json::Value` or log/error
  messages. The writer's buffers remain the caller's responsibility.
* After Microsoft token rotation, downstream failure returns the already-owned
  refresh token by moving it into `exchange_failed`; no fallback token clone is
  allocated. Intermediate Microsoft access credentials are not returned.

This is not a universal memory-erasure guarantee: compiler temporaries, registers,
OS/TLS buffers, allocator abort paths, downstream writers and transport adapters
are outside the guarantee. Rust's default allocation failure abort is not Zig's
recoverable allocator-error model. A caller must also ensure its outer JSONL
parser and input-buffer lifetime respect this boundary; wrapping the final
string in Zeroizing does not erase a JSON decoder's earlier scratch allocation.

## Verified synthetic compatibility and remaining boundaries

`rust/compat/build_auth_oracle.py` extracts exactly
`5a4310debbcee26b3d2391bab2339f79cd6ec52e` from Git. It changes only the two HTTP
adapter imports to an injected fake; original auth and protocol code remain
unchanged. The runner rejects non-auth methods. `tests/auth_oracle.rs` compares
complete terminal responses and outbound requests: all five success flows,
rotation failure at each downstream step, transport failures, Xbox restrictions,
malformed/escaped parameters and response shapes, Unicode surrogate validation,
and allocation-dependent OAuth error string limits. The latter is observable
Zig behavior: unescaped borrowed error strings bypass its 128-byte allocation
limit; escaped allocated strings do not. Rust preserves this within the overall
64 KiB response ceiling.

Run with `JMCL_AUTH_ORACLE=<built-path> cargo test --test auth_oracle -- --ignored`.
Use a per-worktree target directory, debug=0 and incremental=0 in constrained CI.

Still not proven: live-service behavior, native TLS/network-stack error equivalence,
and extreme scientific-form numeric coercion (Rust f64 versus Zig f128). The
HTTP library's HeaderValue and internal HTTP/TLS allocations do not offer wiping;
the temporary header assembly buffer is wiped and the header is marked sensitive,
but no universal erasure claim is made. No actual credentials or auth network
requests are used by any test. Production dispatch integration belongs to the
shared protocol owner; outer JSON syntax/Unicode errors must remain
INVALID_REQUEST rather than auth-specific INVALID_PARAMS.

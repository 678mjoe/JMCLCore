# Authentication port boundary

This module implements the five authentication RPC state machines against an
injected `Transport`. It does not persist credentials, create an account vault,
add a login CLI or add PKCE endpoints absent from the pinned Zig core. Its tests
use only synthetic scripted responses; there is no production HTTP adapter here.
A caller integrating an adapter must enforce the request's timeout and bounded
response size while reading, not merely after an unbounded read, must not forward
credentials on redirects, and must avoid logging request bodies/bearer values.

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

## Compatibility work still required

The pure state machines and scripted chains are tested, but these tests do not
constitute a live Microsoft-service compatibility claim. A bounded HTTP adapter,
outer dispatch wiring, and side-by-side injected-transport oracle fixtures are
separate acceptance gates. Exact numerical coercion of extreme scientific-form
u64 values needs differential coverage (Rust f64 versus Zig f128). No known
incompatibility is silently counted as completion.

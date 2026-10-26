# Metadata response finalization order

`HttpTransport::get` now delegates its response body/status classification to
one private helper used by injected-reader tests. This is an ordinary metadata
GET adapter change, not an installation retry or archive operation.

Pinned source `5a4310de`, `src/service.zig:4136–4182`, consumes into a fixed
`max_bytes + 1` writer. Body/read failure is handled before non-success status;
a completed oversized body becomes `ResponseBodyTooLarge` before status is
examined. The previous Rust inline implementation returned status first and
also rejected an advertised Content-Length before reading the bounded body.

Before changing behavior, the **existing inline implementation was mechanically
extracted**, still called by the production `get` method. All three injected
reader tests failed on that unchanged implementation, including a non-success
response returning `UnsuccessfulHttpStatus` instead of `ResponseBodyTooLarge`.
The final helper reads at most `max + 1` bytes, keeps the existing read-error
mapping, checks actual size, then checks status. Header length is informational;
HTTP framing validation remains the response reader's responsibility.

`version_http_tests.rs` checks both successful/non-successful status, empty and
exact-boundary bodies, absent/small/large advertised lengths, timeout and other
read failures, and 0/1/3/7-byte limits. One-byte-chunk readers count delivered bytes
and calls, proving that neither an oversized body nor an error beyond the sentinel
causes additional reads. These are **source-derived finalizer contract tests**,
not runtime Zig HTTP parity, native TLS, redirect, or real-server acceptance.
Malformed header/body combinations are injected-reader controls and do not claim
that a native HTTP reader accepts those wire messages.

No test constructs an HTTP client, opens a network connection, invokes a resolver
or install/retry pipeline, or uses provider credentials. Existing HTTPS-only,
timeout, redirect, user-agent, and HEAD policies are unchanged. The separate
content transport and credential-origin restrictions are untouched. Existing
coarse `NetworkError` mapping for non-timeout reads is retained, not newly claimed
to match every Zig transport error.

```sh
cargo test --locked --lib version::http_tests
```

The tests are portable and included in ordinary library tests. Local Linux
results alone do not establish native Windows/macOS execution.

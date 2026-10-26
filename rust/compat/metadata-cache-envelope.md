# Persisted metadata-cache envelope compatibility

Scope: `version.rs` cache decoding only. The requested URL, SHA-1 key, write
layout, inclusive freshness TTL, 24-hour stale fallback, future timestamp rule,
immutable policy and eviction behavior are unchanged. HTTP transport, dynamic
RPC numbers, planners and acquisition are not part of this change.

The pinned `5a4310de` implementation in `src/install/metadata_cache.zig` parses a
typed `u32` version, signed `i64` timestamp and byte-slice URL/payload. Ordinary
serde integers rejected valid envelopes such as `v: 1.0`, and Rust consequently
removed a cache entry that Zig retained. The cache now retains numeric lexemes
for the existing binary128 typed coercion and decodes typed-u8 byte arrays.
Signed bounds are checked before conversion; exactly one sign is permitted.
A stored non-UTF-8 URL cannot equal a requested UTF-8 URL and remains a miss.
Serialization is unchanged: normal strings stay strings and opaque payloads
remain byte arrays. Malformed strings in ignored fields cause the same cache
miss and eviction as malformed known fields; valid unknown fields remain ignored.

`metadata_cache_envelope.rs` compares the actual exported Zig cache lookup with
Rust `Service::cached` on separate owned directories. Rust's injected transport
only records calls and returns a network error: no HTTP client is constructed.
This makes fresh hits, stale fallback attempts and misses observable. Each case
checks retained/deleted entry state, exact peer and sibling bytes, permissions,
and real file identity on Unix/Windows. No artifact, archive, JVM or resolver
is invoked. Invalid entries may be removed only within each disposable fixture.

Run with Zig 0.16.0 and an unchanged checkout at the pinned revision:

```sh
JMCL_ZIG_SOURCE=/path/to/pinned/source \
  cargo test --locked --test metadata_cache_envelope -- --include-ignored --nocapture
```

The 72-case corpus includes version/timestamp bounds and quoted/float grammar, typed
payload/URL byte elements, malformed envelopes, unknown-field JSON strings,
future timestamps, TTL equality, 86400/86401-second stale boundaries, immutable
entries, payload limits and failed-refresh retention. Direct assertions also
verify the intended freshness results, not just differential agreement.

Signed-boundary cases use an equal `now` so the original signed subtraction
cannot overflow. This does not verify or correct the separate original extreme
cross-boundary arithmetic behavior. Native platform execution is a CI gate;
local Linux evidence alone is not a native Windows/macOS claim. Live TLS,
redirects, response-body/status precedence and upstream acceptance remain
separate from this cache-only test.

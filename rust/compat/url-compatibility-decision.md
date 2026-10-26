# COMPAT-URL-001 — accepted URL representation/normalization deviation

**Decision:** retain the existing Rust URL/HTTP backend. Opaque non-UTF-8 download
URLs remain unsupported; existing Unicode and dot-segment normalization is an
explicitly accepted compatibility deviation. Do not add a dedicated byte-preserving
transport, silently substitute bytes, or describe these cases as wire parity.
This is a compatibility choice, **not an original Zig defect** and not a waiver
for unrelated errors, parser behavior or security changes.

## Named, reproducible boundary

Pinned baseline: Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, Zig 0.16.0.
Rust construction baseline: reqwest 0.13.4 / url 2.5.8. The fixture records all
32 inputs as byte arrays and separately asserts each exact source and Rust result.
Expectations are fixed; the test never regenerates or blesses results.

Six named cases carry COMPAT-URL-001:

| Case | Source primary target | Retained Rust construction |
|---|---|---|
| `unicode_path` | raw UTF-8 `/é?q=é` | `/%C3%A9?q=%C3%A9` |
| `dot_segments` | `/a/../b` | `/b` |
| `escaped_dot` | `/a/%2e%2e/b` | `/b` |
| `raw_ff_path` | `/` + byte FF + `.jar` | cannot enter String URL construction |
| `raw_ff_query` | `/a?q=` + byte FF | cannot enter String URL construction |
| `surrogate_path` | `/` + bytes ED A0 80 | cannot enter String URL construction |

The test's `NonUtf8String` label identifies its input-representation boundary;
it is **not a new public Core error code**. Production URL fields and errors are
unchanged. The user decision accepts the retained behavior, not an additional
reject-all-Unicode policy or automatic encoding of arbitrary bytes.

Eight strict target controls cover ordinary ASCII, existing percent escapes
(including case), slash escapes, malformed percent spelling, empty paths,
query-only URLs and fragment omission. Eighteen further host/scheme/control-byte
rows are **construction-only diagnostics**, not approval to pass unsafe input
through provider validation. In particular, construction accepting an HTTP URL,
credentials, whitespace or control byte is not evidence that the provider or
transport accepts it.

## Source reachability and security boundary

Original content resolution retains the provider URL (`src/service.zig`), copies
it into the install plan, and passes it to `downloadOnce` in
`src/install/installer.zig`. Zig's primary `std/http/Client.zig` formats path and
query with `std.Uri.writeToStream`; the checked-in pure fixture uses those flags.
This demonstrates source construction, **not successful HTTP transmission or
remote server acceptance**. Source fallback empty-path/query formatting is a
separate behavior; no fallback transport is exercised by this corpus.

No production source changes accompany this decision. Existing HTTPS-only client
settings, TLS certificate verification and URL-derived host/SNI handling remain
unchanged. Metadata credential-origin checks and keyed redirect restrictions in
`rust/src/content/http.rs` remain separate from credential-free artifact URLs;
artifact redirect bounds in `rust/src/install/mod.rs` remain unchanged. Nothing
here authorizes relaxing these policies or bypassing validation. Their live
network behavior is not established by a constructor test.

## Executable evidence

`url_construction_contract` calls only UTF-8 validation, `reqwest::Url::parse` and
inert `Request::new`. It never constructs an HTTP Client, sends, resolves hosts,
negotiates TLS, follows redirects or accesses a provider. Its optional Zig fixture
calls only URI parse/format/getHost, reads an owned input file and writes JSON.
Every original/Rust expected record is compared exactly. Strict target controls
also compare source and Rust target values directly; there is no wildcard
normalization or blanket expected-failure branch. Input bytes and timestamps
are checked after each paired case.

```sh
cargo test --locked --test url_construction_contract
JMCL_ZIG_COMPILER=<zig-0.16.0> cargo test --locked --test url_construction_contract \
  -- --include-ignored --nocapture
```

Native execution of this new corpus is a separate CI gate. This accepted deviation
does not close Version/CurseForge malformed-JSON diagnostic debt, arbitrary provider
live acceptance, or other documented replacement conditions.

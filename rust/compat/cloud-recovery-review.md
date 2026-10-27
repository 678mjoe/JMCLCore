# Cloud recovery routing review

Reviewed source: `9c8d3314c6e524d76188f9f6484ed2fdaed70023`, tree
`7b9740a1f249959e821314232b44700a9c7263ff`, relative to durable checkpoint
`8ca8770c581552554a65217989329059d008ec85`. Oracle source remains
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`; no Zig runtime was available.

The static validator initially rejected three stale file digests. Each changed
file was reviewed before repinning; unchanged pins, selectors, methods, platform
guards and protocol order were preserved. This audit maps routes, not runtime
coverage. Evidence paths remain `referenced_not_run` in the static map.

- `version.rs`: the same version handler owns list/get/resolve/install.plan.
  Only list uses `listing_manifest` and the byte formatter. Existing cache URL,
  2 MiB bound, TTL 600, transport, eviction and write functions are unchanged.
  Acquisition still calls the original String manifest method.
- `instances.rs`: the same four methods and desktop platform guard remain.
  Literal `validate:false` alone selects the private byte preflight/writer;
  omitted/true/null/malformed validation retains the original decoder and
  resolver path. Get/list/identity and delete preflight use the byte view.
  Root opening, requested/stored identity and error mapping remain ordered.
- `manifest_ordered.rs`: Text is generic for the byte view; existing instance
  and server operational models retain Text=String. Shared cursor, duplicate,
  enum/number and required-field checks remain in the same parser.

Aggregate review: public operational String APIs remain available; metadata ID
UTF-8 arrays intentionally normalize to Strings, while opaque acquisition IDs
remain unsupported. Explicit-false creation uses a mechanically extracted
anchored writer. Content list/toggle/remove/adopt change only manifest gates;
world list/show forwards version bytes only. Mutation/adoption/removal bodies,
CAS transactions, digest locks, no-follow helpers, HTTP/URL policy and default
resolution are unchanged. No public method names were added. Intended wire
changes are source-defined string-or-array fields, not lossy text coercions.

The regression allowlist contains only previously reviewed pure parsers,
guarded explicit-false creation, owned read/delete/content/adoption fixtures,
and owned metadata caches with rejecting/counting transport. Shared server
manifest tests parse in memory only; they do not invoke server lifecycle. Static
regressions mentioning Java only mutate audit JSON and do not run Java or Core.
No aggregate Rust test invocation, real network/provider/account operation,
Java/game execution, archive operation or GC fixture is included.

Runtime results are recorded separately with exact source/tree/binary hashes and
each exact test name. Native Windows/macOS and Zig differential evidence remain
pending. Retained CAS digest-lock files are an accepted layout difference; this
review does not authorize replacement or claim whole-disk parity.

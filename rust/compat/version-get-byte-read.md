# Standalone version metadata byte-ID reads

RPC version.get and CLI version show use a private byte request/result. Whole
parameter schema validation precedes the empty-ID check and client construction.
Only metadata's top-level ID normalizer varies; the complete existing schema,
defaults, nested checks, duplicate tracking and numeric handling are shared.
String Service::get and resolve/install-plan/default Create callers retain the
String metadata parser and public APIs. The prior fetch/hash/retry body was
mechanically extracted and remains shared without new source fallback.

JSON output renders an opaque ID as bytes, UTF8 as text. Text show and not-found
diagnostics write the ID bytes. A narrow Unix OsString adapter claims only raw
version show arguments before the UTF8 argv fallback; no ID-derived file paths
are used. Cache filenames remain SHA1(URL). Native Windows opaque argv remains
outside this implementation; RPC byte arrays do not require opaque native argv.

Tests call pure native-argument parsing and the shared formatter with an injected
owned-cache service, never the actual CLI HTTP constructor. Fake transport
rejects every unqueued request. Selected unsupported URL bytes retain the prior
COMPAT-URL-001 boundary; opaque SHA1 is not ignored. No artifact acquisition,
resolver/planner, Java/game, accounts, archive or server lifecycle executes.

The static version route now records the private request/read schema edges; this
is not runtime coverage. Existing compound-malformed metadata error-order limits
and opaque fields other than the top-level ID remain separate. Zig/native
execution equivalence is not claimed.

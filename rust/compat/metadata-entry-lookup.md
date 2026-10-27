# Metadata entry lookup recovery

The listing view now selects the first byte-equal ID only after parsing the
entire manifest. Unselected opaque IDs, URLs and hashes no longer reject an
ordinary selected ID. Selected fields retain exact bytes.

`Service::get(&str, Source)` uses this lookup while retaining its public String
signature and String metadata return. The original `manifest` method remains
unchanged. URL rewriting, 4 MiB immutable metadata caching, hash-before-parse,
cached hash-mismatch eviction/refetch, and final metadata-ID checks keep their
existing order. Pinned source: `service.zig:1196` at
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

COMPAT-URL-001 applies only when the selected URL cannot enter the existing String
transport: it still returns the existing `UnexpectedToken` classification before
metadata transport. No byte substitution or percent encoding is added. An opaque
SHA1 is present and fails verification; it is never treated as absent. Neither
case excuses rejecting unrelated opaque entries.

Tests use only in-memory parsing, owned metadata cache envelopes and strictly
queued fake metadata responses. They cover first-match and parse ordering,
unchanged fresh caches, selected URL rejection, fresh/cached hash failures,
retry cache replacement, and final metadata-ID mismatch. No artifact content,
resolver, planner, provider request or installer is executed. Expectations are
source-derived; Zig/native equivalence and existing compound-malformed metadata
parser error-order gaps remain unverified.

The static version handler association is unchanged. Its source digest and new
fixture reference were refreshed after review; static evidence is not runtime
coverage. Acquisition callers receive only the intended lookup correction.

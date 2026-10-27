# Conditional vanilla install.plan read integration

Only requests with all loader fields absent/null select the new route. Empty,
malformed, selected and conflicting loader fields keep the old complete path.
No new public vanilla-only error exists. Loader opaque-input gaps and the
pre-existing Rust conflict-before-host versus Zig host-before-conflict ordering
remain separate; this is not full install.plan parity.

Raw eligible requests use the existing structural JSON scanner, dynamic byte
number decoder and enum rules. Unknown overflow numbers are not eagerly turned
into serde Values. The protocol's Unicode and duplicate-key checks remain ahead
of dispatch. Full schema precedes empty-ID rejection. The Value/UTF-8 CLI adapters
share the raw decoder. Native raw argv is unchanged.

Each attempt preserves manifest-first lookup, metadata cache/hash/retry, complete
metadata parse and ID comparison, optional asset fetch/hash/parse, then component
validation and pure planning. BMCLAPI failures retry the whole attempt with
official metadata/assets but original download and output source. Assets retain
the existing direct 16 MiB transport boundary; no new asset cache is introduced.

Public String Service planning, resolver, installation, launch and receipt APIs
are unchanged. The new read plan is serialized only; no paths are materialized.
Java-major metadata is copied without runtime selection. Nested metadata Strings,
loader ASCII policy, URL-B and CAS lock policy remain unchanged. Existing metadata
compound error-order limitations and Zig/native execution evidence remain pending.

Tests use owned explicit caches, synthetic JSON and URL/limit-checked injected
transport only. Fresh seeded caches remain unchanged. The known cached hash
mismatch fixture permits exactly one entry replacement and cache directory mtime
change while preserving other bytes, identities, permissions and hardlink peers.
No real provider, artifact, archive, Java/game/server/account or loader resolver
execution is evidence for this slice. Static audit mapping is not runtime coverage.

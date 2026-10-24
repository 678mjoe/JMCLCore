# Version metadata and Vanilla planning slice

Reference: Zig Core `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
This slice is not a complete Core replacement. Loader selection remains explicitly
`NOT_IMPLEMENTED`; it is not silently treated as Vanilla. No live Minecraft,
account or public-network acceptance is implied.

Implemented domain functions include normalized Mojang manifests/version metadata,
asset indexes, source URL rewriting, Vanilla client/server install planning,
classpath/native rules, asset projections, deduplication/conflict checks, bounded
metadata HTTP and the existing URL-SHA1 cache envelope/TTL/stale fallback format.
RPC covers `version.list`, `version.get`, Vanilla `version.resolve` and Vanilla
`install.plan`; CLI covers `version list`, `version show` and Vanilla `install plan`.

## Integration

- Add `pub mod metadata; pub mod plan; pub mod version;` to `src/lib.rs`.
- Dependency: `reqwest = { version = "=0.13.4", default-features = false,
  features = ["blocking", "rustls"] }`; `serde_json` needs `preserve_order`.
- `version::handle(method, params)` returns an optional result containing
  `Response { source: metadata::Source, result: Value }`. A success envelope must
  serialize fields in order `protocol,id,event,source,result`; an error retains
  the normal error envelope without a source field.
- `version::cli(args,stdout,stderr)` returns `None` for other commands; call it
  before the current generic `version`/`install` fallback.
- `version::Service<T: Transport>` can be reused by Java/loader slices.
  `get(id,source)` is direct, `get_preferred` adds BMCLAPI-to-official fallback.
  `Service::new(transport,".jmclcore/metadata-cache")` uses the existing cache.

## Offline evidence

`version_check.py --zig PATH --rust PATH` seeds disposable old-format caches and
compares 106 RPC/CLI cases byte-for-byte without fetching public metadata.

Compile the read-only adapter against the pinned oracle source, not an arbitrary
future checkout:

```sh
zig build-exe --dep jmcl_core \
  -Mroot=rust/compat/metadata_oracle.zig \
  -Mjmcl_core=/path/to/pinned-oracle/src/root.zig \
  -femit-bin=/tmp/jmcl-metadata-oracle
JMCL_ZIG_METADATA_ORACLE=/tmp/jmcl-metadata-oracle \
  cargo test --manifest-path rust/Cargo.toml --test metadata_differential -- --include-ignored
```

The adapter makes no network requests. The test compares normalized metadata,
manifest and asset output plus normal/error client plans and a server plan across
40 fixtures/combinations, including Fabric/NeoForge schemas, overlays, error cases and read-only ZIP lookup. Ordinary unit tests cover malformed fields, duplicate
known keys, mirrors, path/hash safety, deduplication, cache expiry/stale fallback,
corrupt cached-body refetch and parameter rejection without network calls.

## Remaining acceptance

Download execution and
progress, native extraction, actual HTTP proxy/redirect/TLS failure behavior,
all real upstream schemas, and exhaustive malformed-JSON error precedence are
not established by this slice. HTTPS verification is never disabled. Fixture
success is not blanket parser/transport parity or proof that all error combinations
match the original. Native multi-platform execution remains the integration CI gate.

## Fabric extension

Fabric profile validation, Maven coordinates, supplied integrity or immutable SHA-1
sidecar/HEAD-size caches, parent-first argument merge and loader-first libraries
are implemented. `Service::resolve[_preferred](id,Option<fabric_loader>,source)`
returns merged metadata; `plan_loader(id,loader,source,target)` tags Fabric plans.
`Transport::head_size` is added with a default error for older test transports;
HTTP HEAD rejects redirects, GET follows at most three, both verify HTTPS and time
out after 30 seconds. Offline end-to-end fixtures exercise cached sidecar + HEAD
size instead of making network requests. All loader families have metadata paths; see Forge extension below.

## NeoForge extension

`resolve_neoforge[_preferred]` and `plan_neoforge` now read bounded installer ZIP
profiles, validate parent/profile IDs, derive patched-client paths and add installer
and processor-library download extras. Resolution does not run processors. JSON
cache envelopes accept the original string-or-byte-array payload representation
(binary installers are byte arrays when not valid UTF-8). `flate2 =1.1.9` is used
for bounded raw Deflate; ZIP64/encrypted/unsupported entries are rejected. The
read-only installer lookup intentionally retains original first-name-match and
no-CRC-validation behavior; it is not a filesystem extraction API. Native/pack
extraction has separate safety requirements. Synthetic deflated installers are
covered end-to-end; no installer was downloaded or executed.

## Forge extension

Modern processor profiles, legacy `versionInfo` installers and processorless
legacy profiles all resolve and plan. Modern generated patched artifacts are
excluded from downloads; old universal coordinates gain their classifier. Legacy
HTTP repositories upgrade only through the pinned allowlist. The alternate
installer Maven URL is tried only for unsuccessful HTTP status, never for
network/timeout errors. Legacy library hashes prefer the first declared checksum,
otherwise a bounded sidecar; HEAD verifies declared sizes. No legacy installer
extras are invented. Fixtures cover all three families through CLI/RPC, including
BMCLAPI source envelopes. `resolve_forge[_preferred]` returns metadata and its
actual `forge`/`forge_legacy` kind; `plan_forge` retains that kind. Processor
execution, runtime download and real-upstream interoperability remain separate
acceptance gates.

## Typed numeric lexemes (wave 4 compatibility repair)

Metadata schema integers now retain their raw JSON numeric token until typed
coercion. Both raw and quoted `1.0000000000000001` reject with `InvalidNumber`,
while raw/quoted `18446744073709551615.0` decode to `u64::MAX`, matching Zig's
binary128 intermediate. Typed u8/u16/u32/u64 fields delegate to the shared
`json_number::unsigned_text` helper (integration prerequisite: numeric helper
commit `7b5fb57`, integrated as `5c772fc`). No global serde precision feature is
changed. Dynamic RPC `Value` parsing/coercion remains separate and unchanged.

The private typed tree preserves object insertion order, duplicate tracking and
unknown-field dropping. Numeric tokens do not become string field values, and
parsing remains nesting-bounded. Sixteen additional raw-source oracle cases cover
fractional precision, binary128 rounding, huge exponents and integer width edges;
the prior 40 pure cases and 106 end-to-end cache-backed cases remain acceptance
gates. This does not claim exhaustive malformed-field error precedence parity.

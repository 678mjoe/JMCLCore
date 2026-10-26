# Ordered persisted instance/server manifest parsing

Private `instances/manifest_ordered.rs` decodes the four existing manifest types
through a sealed trait and `Wire<Name>`. It replaces eager whole-document
RawValue/Unicode preflight with field-ordered scanner and typed validation.
Earlier enum/type/number/duplicate errors now precede later malformed syntax;
earlier malformed ignored strings still win. Required fields are checked before
trailing-token validation, while post-parse semantic validation remains after it.

The existing content cursor is reused with minimum crate-private visibility only:
no scanner body changes, no provider/auth/dynamic parser changes. Existing public
serde derives, serialization, construction and `validate` APIs stay unchanged.
Source enum canonicalization is shared with the old serde helper (including
ordinal signs/underscores and the special `-0` rejection). Schema numbers retain
typed numeric grammar rather than binary64 conversion. Unknown values retain
lexical validation and huge numeric spelling without numeric coercion.

Instance/server operational names remain String-only; read names remain
DisplayName typed byte arrays. IDs, versions, loaders and server java_path retain
existing String-only boundaries. No operational byte-field acceptance is added.
Real common fields and server accept_eula/java_path are converted explicitly;
there are no placeholder values. Instance schemas still ignore server-only keys.
The four existing wrappers preserve request-ID validation, 64 KiB size limit,
decode, semantic validation and stored-ID matching in the same order.

Pinned source: 5a4310debbcee26b3d2391bab2339f79cd6ec52e,
`src/instance_store.zig` Manifest/load and `src/server/server_store.zig`
Manifest/load; typed parsing uses ignored unknown fields and max_value_len 64 KiB.
Baseline executable b132cd/0212c75c exposes 30 ordering differences in the strict
73-pair instance/server JSON-show corpus. Exact 64 KiB input, a 65,000-digit ignored
number and depths 128/129/256 are accepted by both engines; no tighter parser
limit is invented. Four-type pure tests independently retain size error contracts.

The original expanded baseline has 74 pairs and 31 differences: one is a separate
server CLI file-read boundary mapping `TooLarge` instead of `ServerManifestTooLarge`
above 64 KiB. This is NOT fixed, normalized or accepted by the parser change.
`--oversized-server-only` retains that single strict failing diagnostic probe;
the default corpus explicitly excludes exactly that case. Raw baseline records
and full inode/mode/mtime/link-count/file-byte snapshots are retained separately.

```
cargo test --lib instances::manifest_ordered
python rust/compat/manifest_ordered.py --zig <pinned-core> --rust <rust-core> \
  --oracle-source <pinned-source> [--output <raw-results.json>]
# Separate read-limit regression (closed by server-manifest-read-limit.md):
python rust/compat/manifest_ordered.py --zig <pinned-core> --rust <rust-core> \
  --oracle-source <pinned-source> --oversized-server-only
```

Only pure parser buffers and owned read-only metadata paths are executed. No
create/delete/adoption, lifecycle, Java, archive, provider or network operations.
Non-Linux server cases exercise only the existing unsupported platform gate;
instance parsing and portable pure tests remain active. Native acceptance is
separate from local Linux evidence.

Linux verification: three new four-type tests, seven prior enum/numeric/display/
server-manifest tests, 73 strict ordering pairs, 344 enum and 458 display pairs
pass. Exact pure regressions pass: MR Project 1,410 and Version 781; registry 359;
CF bytes 275, lexical 128, pages 224/459, typed envelopes 640, Project 247, and
URL 236 source-equal cases plus one explicitly accepted COMPAT-URL-001 case.
Clippy all-targets and diff checks pass. Executable SHA256:
c94a5f6d1022b36cef64e031017a98e5d31e19a7820398d0f4a8c9dd41c2b9e5.

Run-history precision: the initial CF module-filter attempt ran ten passing direct
checks plus `content::curseforge::page_parser_tests::injected_metadata_page_loop_preserves_stop_and_aggregate_rules`
(a passing injected Metadata-only resolver-loop fixture, NOT parser-only). Seven
oracle tests initially failed to spawn literal `zig` because PATH was unset; after
correcting PATH they were rerun by exact fully-qualified names and passed. No live
request was sent. Future runs use explicit per-test allowlists, not broad filters.
At the parser-only head, the oversized-server-only probe remained a strict failing
consumer case with unchanged disk snapshots, outside the 73 passing pairs. The
separate [server manifest read-limit fix](server-manifest-read-limit.md) now closes
that consumer diagnostic; the dedicated one-case probe passes without normalization.

# Install Plan

An install plan is immutable output describing the files required before a
Minecraft version can launch. Generating a plan does not write game files.

## Layout

The current plan uses the standard launcher cache layout:

| Resource | Destination |
| --- | --- |
| Client | `versions/<id>/<id>.jar` |
| Asset index | `assets/indexes/<asset-index-id>.json` |
| Asset object | `assets/objects/<hash-prefix>/<hash>` |
| Library | `libraries/<artifact-path>` |
| Native archive | `libraries/<classifier-path>` |
| Logging configuration | `assets/log_configs/<file-id>` |

Plan generation downloads the selected asset index, verifies its SHA-1, and
deduplicates resource objects by their content-addressed destination.

## Files

Each file contains:

- `path`: validated relative destination under the launcher data root.
- `url`: selected primary download URL.
- `fallback_url`: original official URL when a mirror is selected.
- `sha1`: expected content hash.
- `size`: expected byte count.
- `usage`: roles the cached file serves.

Files are deduplicated by destination. A duplicate destination with a different
hash or size makes planning fail before any download starts.

## Asset projections

Legacy indexes can request `virtual` assets, `map_to_resources`, or both.
`asset_projections` describes the required materialization from a shared object
path to one of these destinations:

- `assets/virtual/<asset-index-id>/<logical-name>`
- `resources/<logical-name>`

Logical resource names are validated as relative paths before entering the
plan. Multiple names referencing the same hash produce one download and
separate projections. The executor should prefer hard links where safe, then
fall back to copies without allowing symlink traversal.

## Classpath

`classpath` preserves applicable library metadata order and places the client
JAR last. Libraries are filtered using Mojang's ordered, last-match-wins rules.

Modern artifacts whose Maven coordinates contain native classifiers remain
ordinary classpath entries unless their metadata has a `natives` map. The
launcher does not infer behavior from artifact names.

## Native extraction

Legacy declarations with a `natives` map select one classifier for the target
OS and Java architecture. `${arch}` is replaced with `32` or `64` before the
classifier lookup. `native_extractions` references the cached archive and its
metadata-defined exclusion prefixes.

The eventual installer must extract these archives into a fresh per-launch
runtime directory, not into the shared library cache. Archive entries must be
validated against traversal and symlink attacks before extraction.

## Security

Plan generation rejects:

- Non-HTTPS metadata download URLs.
- Invalid SHA-1 values.
- Absolute, traversing, Windows device, or otherwise unsafe paths.
- Conflicting files sharing a destination.
- Applicable libraries with missing artifacts.
- Native mappings with missing classifiers.

The download executor must additionally enforce byte counts and hashes while
streaming to temporary files, then publish them atomically.

See `docs/installer.md` for the current executor behavior.

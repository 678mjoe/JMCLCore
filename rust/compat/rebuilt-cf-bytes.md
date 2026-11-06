# Rebuilt CurseForge loader and override byte parity

This is a new implementation from recovered baseline
`a4ad20ac62415397cbe6495ce238bbbcef61e2ad`, checked against fixed Zig source
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`. It is not a recovered copy of the
lost local implementation or evidence that its previous commits were restored.

## Loader chain

CurseForge loader IDs and modpack selection versions retain complete bytes.
Selection splits at the first hyphen, requires 1–128 total ID bytes and a
nonempty suffix, validates only the supported prefix, and compares versions by
bytes. It preserves conflict-before-primary and primary early-return order.
Mrpack dependency ASCII validation and conflict order remain unchanged.

The modpack service constructs a byte-preserving instance manifest. Shared pure
validation/encoding and a crate-internal `create_bytes` adapter reuse the
unchanged anchored instance writer: validate before directory creation, encode
after creating the game directory, and clean up only the new instance on failure.
Summary JSON keeps UTF-8 strings or complete non-UTF-8 arrays. Loader CLI text
uses `write_all`, preserving NUL and invalid UTF-8.

## Override directory chain

The typed overrides field and selected directories retain bytes. A new internal
`overrides_directory_bytes` function appends one slash to that byte directory;
production service calls this full-byte path. Existing public `overrides` and
`overrides_bytes` String-directory entry points remain adapters with their prior
callback and error contracts. `Pack::override_directories` now returns byte slices
because its selection must represent non-UTF-8 data.

The pure `override_relative` helper retains the original sequence: prefix match,
empty directory-record skip, safe-relative-path validation, then the 512-byte
relative-path limit. Entry count, per-file and cumulative byte budgets, archive
read order and atomic private file writes are unchanged. There is no new length
restriction on the override directory itself.

## Bounded evidence and limits

New pre-fix schema tests failed on valid byte-array inputs; their patch and log
were preserved separately before implementation. Tests cover opaque suffixes,
NUL, UTF-8, exact/over-limit IDs, typed-number errors, conflict and primary order,
byte override directories, default selection, path errors, exact prefix matches,
empty records, and path-before-length error order.

The fixed-source Zig fixture uses only schema/selection, manifest JSON and byte
formatting. Override matching/validation is tested using the extracted pure Rust
helper and reviewed against `service.zig::extractOverrides`; no real archive
operation is invoked. Rust manifest writing and injected encoder failure tests
operate exclusively in their own `tempfile` directories. Existing read-fixture
tests likewise use self-owned fixtures. These are not installation tests.

No real installation, download, authentication, archive execution or launch is
performed. Generic `ReadManifest`, `InstallationIdentity`, and downstream loader
installation/launch APIs retain String boundaries. Modpack plans still consume
game version rather than loader version. No native Windows/macOS execution,
full suite pass, complete parity or replacement readiness is claimed.

The static dispatch audit's existing source inventory covers `instances.rs`
(the new crate-internal re-export), but not the other changed implementation
files. This change does not expand or weaken its inventory or CI allowlists.
Independent review must precede the affected digest/reviewed-commit refresh.

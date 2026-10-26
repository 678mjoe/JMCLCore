# Server show manifest-load diagnostics

Pinned `src/cli/server.zig` `runServerShow` writes the original manifest-load error
name. Rust previously delegated through the RPC error envelope, losing this
information: even `MissingField` and `DuplicateField` became the generic text
`failed to load the server`.

The CLI show path now requests an optional private diagnostic from the same
anchored `store::load` call. No second open or read is performed to infer the
cause. Normal RPC dispatch does not request the diagnostic and retains exactly
its existing error code/message. Other CLI commands, root handling, installation
status behavior, and the Linux-only server-management gate are unchanged.

## Evidence

`compat/server_show_errors.py` checks 25 owned ordinary record conditions in
text show, JSON show and `server.get`: 75 exact comparisons. Success cases require
real decoded IDs/names and `installed=false`; matching errors cannot satisfy
those controls. Malformed cases cover required/duplicate fields, null/bool types,
numeric schema errors, semantic names/versions/loader conflicts, invalid IDs,
and missing manifests. A Java-path string is validated as metadata only; no
executable is opened or run. Before and after every invocation the complete
fixture file/directory snapshot, including unrelated hardlinks, must be unchanged.

Linux local tests pass. Native macOS/Windows execution remains a CI gate; there
the corpus asserts the original unsupported-platform error before option or
manifest parsing, not Linux manifest functionality. No lifecycle, process state,
network, installation or archive operation is tested.

```sh
python rust/compat/server_show_errors.py \
  --zig /path/to/pinned/zig-core --rust /path/to/rust-core \
  --oracle-source /path/to/source-at-5a4310d
```

## Separate evidenced gaps

This change preserves the Rust load error rather than redesigning its parser.
The broader persisted byte-string audit found valid array-form manifest fields
still rejected, and non-UTF8 names require Zig's JSON byte-array serialization.
Additionally, combining `source="invalid"` with an empty name exposes an enum
validation-order mismatch: Zig reports `InvalidEnumTag` during parsing, while
Rust currently reaches name validation first. Neither parser gap is accepted as
parity or hidden by this diagnostic correction. The subsequent
[source enum correction](manifest-source-enum.md) addresses the typed enum case
separately, with its remaining compound syntax-order limitation documented. Root-opening and installation
status diagnostics also remain outside this bounded test corpus.

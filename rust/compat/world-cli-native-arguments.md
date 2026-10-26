# Native world list/show CLI arguments

Pinned `src/cli/worlds.zig` parses byte arguments for instance/server worlds
list/show. Rust's global `args()` conversion previously panicked before these
commands saw opaque native paths or IDs. A narrow `args_os` hook recognizes
only these four read routes, only when some argv is not UTF-8. Fully UTF-8
arguments and all other routes retain the existing dispatcher and parser.

The native parser preserves the two source grammars: instance validates its
ASCII ID before options, while server opens its root before validating the
ID. A second show positional is rejected by instance but ignored by server.
Repeated directory options use the final value; recognized options consume
values even when those look like flags. No lossy strings or placeholder IDs
are used. Native root/world values retain Unix bytes or Windows WTF-8. Actual
manifest version data and the existing metadata read views/formatters are
reused. The portable server root keeps its existing flags, with a native-path
delegate; its property-derived three-world list order is unchanged.

This adapter does not intercept mutation/archive or hidden commands. Pure
hook tests verify those routes and all-UTF8 inputs return `None` with no output
without invoking their operations. No operational parser, lifecycle, archive,
Java or network operation is exercised by the corpus. Named alias and metadata
presence lookup policies are not broadened by this change.

`world_cli_native_args.py` runs owned preexisting roots, valid public manifests
and synthetic level.dat metadata with inert readonly hardlinked peers. Its
180 Linux comparisons cover text/JSON list/show, Unicode/opaque root/world/ID,
root/ID/name precedence, empty/repeated/missing/consumed options, unused raw
labels, and source positional asymmetries. Positive metadata/version relations
are asserted; all bytes, modes, inode/link identities and mtimes must remain
unchanged. Before the fix, 136 comparisons differed, all due to Rust argv
conversion panics. The fixed Linux binary passes all 180 comparisons, the
explicit no-dispatch unit, and 448 prior read-only command comparisons
(metadata, enumeration, root errors and manifest names). Library/bin Clippy
also passes. Native platform results require exact-tree CI; Linux proof
is not Windows/APFS acceptance. If the filesystem rejects opaque names with
EILSEQ or Windows ERROR_INVALID_NAME, the corpus visibly reports that physical
success exclusion and compares exact list/show outcomes on the unsupported
path instead. Unicode controls remain mandatory. No cleanup-candidate names
are created. Fixtures are retained without chmod-based readonly cleanup.

```sh
python rust/compat/world_cli_native_args.py --zig "$JMCL_ZIG_ORACLE" \
  --rust target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
```

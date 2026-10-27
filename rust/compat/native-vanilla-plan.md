# Narrow native vanilla planning argv

This closes the source-documented unclaimed argv gap only for Unix non-UTF8
`install plan ID` with vanilla options. The production native dispatcher checks
command/encoding, parses options, and only then invokes the continuation that
constructs transport and runs the existing planning read executor. Tests inject
an owned-cache service into that same dispatcher; no real CLI process or HTTP
constructor is exercised.

Pinned cli/install.zig:525 scans arguments left-to-right: nonempty positional ID,
repeatable --json, last valid source/os/arch wins, invalid occurrences fail
immediately. amd64 and aarch64 aliases normalize exactly as existing String CLI.
Opaque ID bytes are retained; ordinary UTF-8 argv is unclaimed so its old parser
and output remain intact. Unknown/missing/invalid vanilla options produce the
existing usage and exit 2 before continuation invocation.

Encountering a loader option returns unclaimed, including missing/empty/raw
loader values and conflicts. An invalid option before it may return the identical
source argument error. No loader resolver is invoked by the adapter or tests.
Raw loader-backed argv still reaches the existing std::env::args fallback and
may fail Unicode conversion. That limitation is not fixed by this vanilla slice.
The existing version-show adapter, conditional RPC classifier, source fallback,
host preflight, String planning APIs and native-path/materialization policies
are unchanged. Main only registers one additional adapter before its old fallback.

Cached fixtures assert exact String/injected-native JSON and raw-text output,
zero metadata transport calls, and unchanged cache/sibling bytes, inode, modes,
link counts and timestamps. This is Linux parser/injected execution evidence,
not actual native CLI process, Zig runtime or Windows/macOS acceptance.

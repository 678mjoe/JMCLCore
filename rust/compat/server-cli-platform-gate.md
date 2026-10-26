# Unsupported-platform gate before local native server arguments

Pinned source 5a4310debbcee26b3d2391bab2339f79cd6ec52e:
`cli/server_content.zig` and `cli/server_datapacks.zig`, lines 14–17, reject non-Linux
platforms before local operation/argument parsing. The exact result is exit 1,
empty stdout, and `Server management is currently supported only on Linux\n`.
Rust's old OS hook declined non-Linux server routes, allowing invalid UTF8 args to
panic during fallback conversion before this existing platform error.

The separate follow-up gate recognizes only `server mods|datapacks` with
`list|enable|disable|remove` and returns that source result before reading remaining
arguments. It performs no root conversion, metadata/guard access, filesystem action
or process operation. main.rs/hidden dispatch and operational parsers are unchanged.
This is not native server support. Install/adopt/provider routes, other families,
raw operation names and other global argument limitations remain outside scope.

The non-Linux test matrix has 40 source/Rust pairs: ordinary Unicode roots plus
raw root, invalid raw ID, raw project and raw unknown-option values for each of
the eight local family/operation combinations. Each command runs in an owned empty
working directory which must remain empty. No physical raw path or server state
is created. A dedicated non-Linux unit calls only the hook with the same cases.
Unsupported-platform rejection is not acceptance of those raw fields on Linux.

Local Linux evidence: all 1,280 native-root CLI pairs remain exact; focused Linux
hook and three prior argument tests pass, with all-target Clippy/fmt/diff clean.
The dedicated platform test correctly runs zero tests on Linux; Windows/macOS
execution of its 40 pairs remains pending native CI. The rebuilt Linux Core has
the identical SHA256 as before this gate change:
498c4cad286be30f1359c5df3a5c27846400615aef21feaf1834d40162de6b0d.

Run `cargo test --test server_cli_platform_gate` and the platform branch of
`compat/server_cli_native_root.py` on a native non-Linux runner with pinned oracle.
No native execution result is inferred from the Linux checks or source inspection.

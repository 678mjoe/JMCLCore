# Explicit Unix probe executable lookup

The pinned Zig 0.16 implementation searches the **parent process PATH**, not the
PATH supplied to the child environment. Empty components are skipped; absent PATH
uses Zig's default. Search continues through missing/non-directory entries,
remembers permission denial, and does not interpret an invalid executable as a
shell script. A legitimate executable with a shebang still works.

Before this correction, Rust used its preallocated direct-exec callback only on
macOS. Linux `Command` therefore searched the child PATH and rejected owned bare
fixtures which Zig successfully probed. Seven of the original twelve differential
cases failed. The same callback now applies on Unix, and Unix ENOTDIR maps to
`NotDir` rather than `Unexpected`. All executable/environment CString storage is
prepared before fork; the callback only performs execve and captures errno.
Windows code is unchanged.

## Bounded evidence

`java_probe_path.rs` compiles the small `java_probe_oracle.zig` adapter against
exact source revision `5a4310debbcee26b3d2391bab2339f79cd6ec52e` and requires Zig
0.16.0. Sixteen cases compare complete serialized results/errors, with explicit
success metadata or error assertions. Each child gets its own parent PATH;
there is no global test-process environment mutation. Success controls verify
Java major 17, vendor `OwnedFixture`, explicit source and x86_64 architecture.
The fixture rejects uncleared Java option variables and requires its child PATH
to remain `/jmcl-no-child-path`, distinct from the parent lookup PATH. Error controls cover
permission precedence, final ENOTDIR versus ENOENT, and invalid executable format.

Only owned property-printing scripts execute. No candidate discovery, real JVM,
network, installation, acquisition or archive operation is involved. Linux local
execution passed; macOS native execution remains a CI gate. This Unix test is not
Windows PATH-search evidence.

```sh
JMCL_ZIG_SOURCE=/path/to/pinned/source \
JMCL_ZIG_COMPILER=/path/to/zig-0.16.0 \
cargo test --locked --test java_probe_path -- --include-ignored --nocapture
cargo test --locked --lib java::detect::tests
cargo test --locked --lib java::direct_exec::tests
```

Source references: `src/java/java_runtime.zig` `probe`/`probeExecutable`, and
Zig 0.16 `lib/std/Io/Threaded.zig` `posixExecv` (16738–16782). The existing module
route inventory digest must be reviewed when integrating the changed Unix cfg;
the RPC handler routes themselves are unchanged.

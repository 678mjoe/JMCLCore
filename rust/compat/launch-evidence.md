# Client launch slice (experimental)

Oracle: original Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
This slice does **not** replace the CLI or protocol launch handlers yet.

## Evidence

- 35 pure launch-plan fixtures compare the complete JSON representation of both plans (synthetic tokens only). Modern/legacy arguments, exact quoting, JVM defaults, literal extras, placeholder errors, Java mismatch/force policy, limited byte-pattern rules, numeric version ranges, logging replacement, virtual legacy assets and target-specific classpath separators.
- Native benign C child: five process differential cases compare all logs, stream/end framing and terminal outcome fields. Cases cover split-secret redaction, CRLF/empty lines/NUL, 16KiB frames, literal shell-like argv, filtered environment and SIGTERM. PID/duration/RSS are inherently variable; checked by invariants, not compared byte-for-byte.
- Five unit tests for parser/framing/redaction/environment, plus typed protocol event writer checks (base64, global sequence, redacted started argv).
- Linux execution verified. Windows implementation uses literal Command arguments, PeekNamedPipe and GetProcessMemoryInfo; native Windows/macOS execution remains unverified. Local Windows cross-check is blocked before this crate by missing `x86_64-w64-mingw32-gcc` in aws-lc-sys.

## Explicit known divergence: inherited output pipes

Original Zig waits output EOF before waiting on the direct child. A descendant retaining the pipe delays completion even after the direct child exits. The bounded negative fixture proves the old oracle is still running at 200ms, kills **only the oracle PID it created**, and waits for the synthetic descendant's own fixed 600ms lifetime to end. No guessed PID or process-group kill is used.

Experimental Rust waits for direct-child termination, drains currently available output in bounded batches, finishes framing and closes its pipe handles. This prevents hanging on descendant-held pipes, but it can omit later descendant output. The test is a **known-gap test, not a compatibility pass**. User decision on correcting the original bug/contract remains required before replacement.

## Secret ownership limitations

Authentication is borrowed; expanded argv has a single Zeroizing owner. Substitution computes capacity before writing sensitive data; no secret-bearing String reallocation is used. The plan implements neither Debug, Clone nor Serialize. Stream scratch and redaction buffers are fixed-size Zeroizing storage, and consumed regions are wiped. Event writers only receive redacted args/logs.

This does not establish whole-process memory erasure: platform Command internals/OS argv storage are outside this guarantee. RPC request parsing is not wired by this slice. The original substring/template sensitivity policy is preserved, including its limitations; do not claim universal secret discovery.

## Reproduce

Build adapters with Zig0.16 using `--dep jmcl_core -Mroot=rust/compat/launch_oracle.zig -Mjmcl_core=<pinned-repo>/src/root.zig` (and separately `launch_process_oracle.zig`). Use writable Zig caches. Compile `tests/fixtures/launch/child.c` with a native C compiler.

Set `JMCL_ZIG_LAUNCH_ORACLE`, `JMCL_ZIG_LAUNCH_PROCESS_ORACLE`, `JMCL_FAKE_LAUNCH_CHILD` to the three executables. Use a private `CARGO_TARGET_DIR`. Run `cargo test --test launch_plan_differential -- --include-ignored`, `cargo test --test launch_process_differential -- --ignored`, and `cargo test --lib launch::`.

## launch.plan RPC addition

`rpc.rs` implements `launch.plan` with the shared authentication borrowed-JSON decoder, wiping-owned request fields, and a typed direct response writer. `launch_rpc.py` passed 36 exact raw-JSONL comparisons against the pinned binary using preseeded metadata caches and a compiled native fake Java probe. Includes source/policy selection, schema and validation precedence, empty auth, optional session, literal extra arguments, numeric-string resolution, metadata expression errors. No network fetch or real Java/account is used. Runtime/CLI execute remain a separate slice; outer request/input erasure depends on the foundation integration.

### Extended numeric evidence

The shared wiping decoder now matches Zig's checked unsigned integer-string syntax (single sign, internal/repeated underscores, no whitespace/edge underscores, `-0`). `launch_rpc.py` now passes **44** exact offline cases, including eight numeric-string boundaries. This is intentionally not a complete Zig f128 coercion claim: the existing floating-point fallback remains f64 and extreme precision cases are not certified.

## Injected-resolver executor (not a production download path)

`executor::prepare` owns opened root/native handles and a cleanup guard across metadata, projection/native preparation, Java resolution and launch planning. `Prepared::run` supervises a literal-argv process and reports native cleanup status. Native paths come from opened handles (including root rename resistance), never an ambient canonicalization of a caller pathname. Tests execute the benign native child and prove cleanup after environment failure, Java selection/download errors, process spawn errors, explicit guard drop and normal exit.

The resolver is injected so an installed/local Java can be exercised without managed-runtime archive downloads. This slice does **not** expose `launch.execute`/instance launch/CLI as fully implemented; managed-runtime downloading remains unavailable here. It does not retry the separately blocked runtime archive experiment. The inherited-pipe contract decision remains open. Native-host process smoke tests now compile a real Rust executable for the current host, covering Windows/macOS on CI rather than relying on shell-script `.exe` substitutes; only Linux was run locally.

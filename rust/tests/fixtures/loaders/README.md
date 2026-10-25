# ZIG-008 exact observations and independently checked contract

`zig008.json` contains only the 75 engine and 12 RPC mismatches captured in
Windows run 40111376472 / job 120923637973 (rewrite commit `4a7f902`). The
`zig` / `zig_disk` / `zig_receipt` fields are the unchanged pinned Windows
oracle observations. The candidate `rust` fields are validated separately:
`exact_processor_oracle` and `install_processor_protocol_matches_oracle` assert
them during the real pinned Linux differential corpus before comparing to Zig.
No fixture is accepted merely because current Rust produced it on Windows.

Pinned source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`, Zig 0.16.0.
Adapters: `rust/compat/loaders_oracle.zig`, `loaders_install_oracle.zig`.
The latter is offline and cannot download; the Java replacement is a benign
native fixture. No real Java archive, Minecraft installer or account is used.

Run with `JMCL_ZIG_LOADERS_ORACLE`, `JMCL_ZIG_LOADERS_INSTALL_ORACLE` pointing
to those compiled adapters, plus `JMCL_ALLOW_PROCESSOR_HASH_LIFETIME_GAP=1`
for the independently documented ZIG-003 Debug hash lifetime difference:

```
cargo test --test loaders_differential -- --include-ignored --show-output
```

Receipt bytes include the target OS/architecture. Cross-host validation changes
only those **expected** fields to the host's declared target; Windows matching
uses the original captured Windows x86_64 bytes, not a normalized observation.

`JMCL_ALLOW_ZIG008_LOADER_PATHS=1` enables this finite deviation only on Windows
x86_64. Every named case still checks exact records and state. Successful Rust
chains must have the exact bytes/hash/size/order, CAS object, read-only shared
file identity, state, and (for RPC) completion receipt. Failed chains must not
write state or a completion receipt. The 17 equal engine and 12 equal RPC cases
have no entry and remain strict. Named cases that are actually equal also stay
on the strict path; the gate does not force the oracle to fail. CLI remains unchanged. Without the gate all
Windows oracle mismatches remain test failures.

This gate is an explicitly recorded correction, **not** a claim of complete
compatibility. The matrix diagnostic flag only collects evidence and cannot
accept a mismatch. Negative controls mutate every evidence field and leaf,
reject unknown keys/cases, and reject private copies or writable shared files.

Verification on 2026-10-24: Linux pinned-oracle corpus **14 passed, 0 failed,
0 ignored**, with the ZIG008 flag set (it cannot activate a non-Windows gate),
including all 75/12 contract rows and rejection controls; focused test clippy
passed with `-D warnings`. Native Windows execution of the new acceptance gate
is still pending; the observed Windows failures are evidence, not a gate pass.

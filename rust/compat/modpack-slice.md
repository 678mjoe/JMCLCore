# Modpack slice — milestone 3, not a replacement-readiness claim

Oracle: Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

## Implemented surface

- `modpack.install` JSONL and `instance modpack install` CLI adapters.
- Internal Modrinth index and CurseForge manifest inspection/validation (no new inspect RPC).
- First-match bounded ZIP lookup, original no-CRC read semantics, stored/deflate entries.
- Original MR client filtering, first two download URLs, recognized content registration.
- Caller-key CF provider lookup, project metadata cache, optional files, missing-direct-link lookup,
  duplicate path checks, shared content registry validation.
- Shared verified streaming CAS execution and hardlink materialization.
- Original instance creation/rollback boundary, retained verified CAS cache for retry.
- Private atomic override replacement, independent per-directory byte/count budgets.
- No new manifest/receipt/resume formats; game installation remains a later operation.
- Typed ordered stream events. Bounded synchronous event/ack bridge keeps a non-Send writer
  on its owning thread and propagates output failure to workers before terminal success.

## Wiring

Requires `pub mod modpack;` and `pub mod install;` in lib, shared installer commits
`7ab6a3f`, `5ce0b6d`, `0fb3ca3`, `3675744`, `20f7161`, `5b25b19`, `97cf328`,
instance bridge `10269cc`. Shared `install::stream` owns the acknowledged writer adapter.

Before generic unimplemented dispatch:

```rust
if let Some(result) = crate::modpack::rpc::handle(&request.method, request.params, &request.id, out) {
    return result;
}
```

CLI adapter `modpack::cli::run` receives arguments **after** `instance modpack`.
Central protocol/main dispatch is integrated separately. Provider requests and artifact downloads use separate
transports; the caller's API key is not put into plans, events, registry, or CDN requests.

## Evidence

- Pure schemas/provider responses: 105 fixtures against `compat/modpack_oracle.zig`.
- Raw JSONL/CLI: 40 exact byte/status comparisons, plus 32 instance-directory snapshots,
  including empty, cached hardlinked, private-override, unsafe-override rollback, CF empty/optional,
  invalid formats/loaders, absent key, existing directory/malformed JSON, missing pack, params.
- 12 pure archive/planner tests.
- 6 service tests: actual inode sharing, private replacement over a hardlink, cache reuse after
  rollback, missing-file precedence, existing directory alias, writer failure rollback,
  injected CF required/optional/download-unavailable flow with fake keys.
- 3 bounded-writer tests: repeated failure across concurrent emitters, writer panic/receiver
  disconnect, worker panic. Worker termination is checked rather than assuming no deadlock.

Run with a per-worktree target directory, never overwrite another checkout's differential binary:

```sh
ZIG_GLOBAL_CACHE_DIR=/tmp/jmcl-modpack-zig-cache zig build-exe --dep jmcl_core \
  -Mroot=rust/compat/modpack_oracle.zig -Mjmcl_core=/path/to/pinned/src/root.zig \
  -femit-bin=/tmp/jmcl-modpack-oracle
JMCL_MODPACK_ORACLE=/tmp/jmcl-modpack-oracle JMCL_ZIG_ORACLE=/path/to/pinned/jmcl-core \
  CARGO_TARGET_DIR=/tmp/jmcl-modpack-target cargo test --manifest-path rust/Cargo.toml \
  --test modpack --test modpack_differential --test modpack_install --test modpack_wire
```

## Explicit gaps / environment limits

- Live CF/download network parity is unclaimed. No real credentials/accounts were used.
  HTTP/TLS backend error mapping and fallback behavior are not exhaustively matched.
- CF credential-bearing redirects are restricted to the same HTTPS API origin. Zig allows up to
  three redirects; forwarding semantics across origins need a separate decision/security check.
- Override paths now preserve raw Unix bytes through `atomic_replace_bytes`; raw-byte RPC/CLI
  fixtures match the Zig oracle. Windows uses the shared WTF-8 backend, but native execution
  is still required before claiming that platform's complete filename behavior.
- Native Windows/macOS execution is pending. Windows GNU cross-check was attempted but blocked
  in `aws-lc-sys` before checking this crate: missing `x86_64-w64-mingw32-gcc`. This was an
  environment/toolchain limitation, not a successful platform check.
- Shared installer scheduling/retry/stream behavior inherits its separately recorded evidence.
  These finite fixture counts are not a proof that all malformed ZIP/JSON inputs are equivalent.

Clippy was not available in the selected Rust toolchain; `cargo clippy` stopped before analysis.

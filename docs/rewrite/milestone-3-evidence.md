# Third experimental integration checkpoint

This is finite evidence, not replacement approval. The target remains `rust-rewrite`; Zig `main` and the fixed oracle are unchanged.

## Local executed tree

Implementation checkpoint: `14d97ddc9c233df0148cc4b72bf58a8e200a391b`, tree `f88f5a1bb304082e052641f3da508e1a02d7ca92`.
Pinned oracle: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
Rust 1.99.1 formatting and all-target Clippy with warnings denied passed. Linux explicitly ran every test including opt-in oracle tests: **143 unit + 87 integration tests passed; zero ignored**. This is a test-function count, not a method-compatibility count.

The isolated Rust executable SHA-256 was `81a55221cb52dbed395ee89e93ae7bbf9a90ff3fc2120fce8a7b13d1100ffd17`; the pinned Zig executable was `b9e628efc5b7ea34047ad25129a98756b6f8ec5d0be38193aee2620d87a82bfe`.
Additional committed harnesses executed 44 exact offline launch.plan RPC cases, 56 raw store GC RPC/CLI cases, and 494 server cases with zero unexpected differences. Server RPC comparisons are parsed JSON; CLI stdout/stderr/status are exact, with only documented generated backup filename timestamps normalized.

Required fixture adapters use the pinned Zig modules, not a Rust fallback. The content metadata adapter covers parser shapes; its injected service tests do not claim unmodified network transport parity. Native Windows/macOS acceptance of this combined tree is pending CI.

## Scope and remaining gates

The combined tree adds actual static install/prepare, lock-bound CAS publication, local content adoption, injected Modrinth resolution, modpack installation with private overrides and rollback, local/installed-runtime launch planning and a process engine, and Linux server lifecycle/world-set operations. Shared protocol dispatch remains explicit; helpers without a production dispatcher are not advertised as implemented methods.

- Modern Forge/NeoForge processor execution remains explicitly unsupported before filesystem mutation.
- Java runtime installation is not enabled or verified; its separate archive-policy question is unresolved.
- Auth has synthetic typed/wiping parser tests, not a complete production authentication acceptance claim. General allocation wiping and extreme numeric coercion remain separate limitations.
- The exact Windows Zig lock-contention retention case is opt-in experimental evidence, not blocking parity. Strict replacement remains failing. Three oversized RPC buffering cases remain separately allowlisted.
- World archive CRC acceptance and descendant-held launch pipe behavior remain documented differences, not silent normalizations.
- Live provider/download behavior, native platforms, absent RPC/CLI methods and full negative-input coverage remain gates. `full_compatibility` and `replacement_allowed` remain false.

Later documentation commits may record this checkpoint, but a changed implementation must be rebuilt and retested before publishing its evidence as current.

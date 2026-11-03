# Vanilla execute native argv, 2026-11-03

Implementation checkpoint: `d60e32ca3eca277ba700c8455b549fac52346959`.
Base: local integrated checkpoint `9d9c222f60d7fdcffd2eab91561ce1aff4e5c686`.
Neither this record nor this local branch authorizes replacement of production main.

## Proven gap and bounded correction

The prior executable panicked with exit 101 in `std::env::args()` for a raw
non-UTF-8 version ID even when a trailing `--directory` had no value. The pinned
Zig oracle returned usage and exit 2 without touching disk. This red comparison
was retained before implementation.

The native argv pre-dispatch bridge now passes vanilla IDs, install roots and
artifact-store roots as exact native bytes to the existing vanilla completion
controller. It validates option pairs, source/target tags, numeric ranges and Java
policy before any transport construction. Repeated options retain original order;
an invalid earlier value is not rescued by a valid later one. `--java-policy` is
validated, but no Java acquisition or processor path is added for vanilla.

Ordinary UTF-8 arguments keep their existing route. Valid loader selections still
use the legacy route; non-UTF-8 Fabric/NeoForge/Forge execution remains an explicit
unsupported gap, not a completed feature.

## Local evidence and boundaries

- Seven focused bridge tests cover native argument encoding (portable Unix/Windows
  fixture definitions), default/range/alias/repeated-option handling, unclaimed routes and three
  Linux injected execution scenarios.
- The synthetic success case verifies raw byte identity in the receipt, exact complete CLI summaries and progress on both fresh and cached runs,
  immutable hardlinked CAS publication, and a second cached run without another
  file fetch. Failure cases verify no fetch on a blocked root and preservation of
  an existing receipt after a failed transfer. All inputs and destinations belong
  to isolated temporary fixtures; payloads are inert text, not Java or game code.
- Twenty-four POSIX raw argv invalid-option comparisons pass against pinned Zig,
  with exact exit status/stdout/stderr and no filesystem writes. Reproduce with
  `python rust/compat/execute-byte-cli.py --zig PATH --rust PATH`.
- Combined local vanilla suite: 23 passed, 0 ignored. Existing plan suite: 46
  passed, 0 ignored. Offline locked binary build, all-target Clippy with warnings
  denied, and formatting check passed.
- Static audit tests: 22 passed in each of normal and optimized Python. The static
  inventory remains 106 names, with no full compatibility or replacement claim.

Pinned oracle revision: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
Only Linux was executed. Windows/macOS fixture execution, live providers, genuine
Minecraft installation/launch, complete loader byte paths and full replacement
acceptance remain outside these results. Original main, previously published
resume branch and the preserved local integrated branch are unchanged.

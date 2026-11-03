# Native instance read CLI, 2026-11-03

Implementation checkpoint: `4986405a62a5a80f650cdbe917d1eeb4297443e8`.
Base: local execute checkpoint `f93a9ded225f31aaee40d30cf7348989e019adf7`.

## Bounded correction

The old executable panicked (exit 101) when `instance list` received a non-UTF-8
instances directory. The pinned original returned an ordinary listing. A retained
red comparison demonstrates this difference. A native argv bridge now handles
only list/show requests containing non-UTF-8 arguments. Other commands and all
UTF-8 requests retain their existing routes. No create/delete/install/launch,
Java, archive, provider, authentication or network path was added or exercised.

The bridge validates all arguments before opening a root, reuses Root::open_native
and byte-aware manifest readers, and writes exact native path bytes in text or a
complete byte-array path in JSON. Its display join follows Zig's naive join,
including Windows drive-relative roots; operational paths remain OsStr.

**Read commands are not completely mutation-free:** as in the original, a valid
request creates a missing instances root. An invalid request must not create it.

## Finite Linux evidence

- Four new pure tests cover malformed IDs/flags, duplicate options, flag-shaped
  path values, unclaimed routes and Unix/Windows display-join spelling.
- Full existing instance unit selection: 43 passed, 1 pre-existing ignored
  source-pinned schema test. That ignored test is not new execution evidence.
- Offline locked build, all-target warning-denied Clippy and formatting passed.
- 37 paired CLI comparisons passed with exact status/stdout/stderr and unchanged
  owned directory snapshots. Includes opaque roots, repeated separators, dot
  segments, list ordering, invalid manifest skip, text/JSON output, blocked files,
  final symlinks, repeated options and invalid arguments before root creation.
- Two further paired cases created only their expected isolated empty roots:
  list succeeded, while show reported the missing instance after creating root.
- Static audit regression tests passed in normal and optimized Python (22 each).

Reproduce from the repository root:

```sh
python rust/compat/instance-read-byte-cli.py --zig PATH --rust PATH
```

Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
Only Linux native execution was performed. Windows join assertions are pure
byte-function evidence, not native Windows filesystem/argv acceptance. No full
compatibility, replacement, live deployment or remote publication is claimed.

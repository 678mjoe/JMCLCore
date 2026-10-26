# Raw native `--file` arguments for local content mutations

The executable now inspects OS arguments before its existing UTF-8 dispatcher.
Only exact local `instance <content-kind> enable|disable|remove` routes and Linux
`server mods|datapacks enable|disable|remove` routes are intercepted, and only
when non-UTF-8 arguments are values following `--file` and every other argument
is UTF-8. Valid UTF-8 invocations keep their existing dispatch path. Hidden server
lifecycle dispatch remains on its original path before the ordinary locked I/O.

The client uses one local argument parser and execution path for text and native
file arguments. The server keeps its existing String parser unchanged, adds a
small local-only parser using the same ID/default validation, and shares parsed
execution/lifecycle locking. There is no sentinel filename, lossy conversion,
network/parser bypass or global CLI argument refactor. Windows converts exact
UTF-16 to WTF-8 using the same checked roundtrip representation as enumeration.
Unused crate-private String-to-byte mutation adapters are removed; public String
Root methods and existing wire APIs remain unchanged.

## Evidence

Pinned oracle: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
A no-match raw `--file` probe returned Zig exit 1 and a normal diagnostic, while
baseline Rust panicked in `std::env::args` with exit 101. The expanded physical
fixture also failed against an isolated checkout of parent `1afcdf6` with only
the new test copied in, before passing with this implementation.

Final Linux evidence:

- `content_cli_os_bytes`: 2 tests, **144 exact paired CLI calls** for all four
  client/two server families and three operations. Raw no-match, duplicate raw
  or UTF-8 last value, empty duplicate, missing value, unknown option, conflicting
  selector and consumed-flag precedence match; registry bytes/identity unchanged.
  This corpus does not require physical raw filenames, so it is mandatory even
  on filesystems that reject them. The gating control confirms unrelated routes,
  fields, hidden lifecycle and all-UTF8 calls are unclaimed and produce no output.
- `content_mutate_bytes`: adds **486** raw native CLI mutation pairs, for **1953**
  total paired steps. Exact output/disk state, shared readonly peer/CAS identity,
  raw unrelated records, missing files/parents and enabled/disabled removal pass.
- Prior content RPC (7), CLI (4), dynamic selector (1 / 558 pairs), raw adoption
  (2 / 73 pairs) and raw registry listing (5) tests remain green.
- Formatting, all-target Clippy and diff checks pass.

Windows physical WTF-8 behavior still requires native CI. The argument-only
corpus uses a native unpaired surrogate; Unix physical tests separately cover FF
and WTF-8 bytes with explicit EILSEQ-only capability exclusions. No data is parsed
as an archive; all files are owned disposable opaque fixtures. No network, Java,
real user data or workload is invoked.

```sh
cargo test --manifest-path rust/Cargo.toml --test content_cli_os_bytes --test content_mutate_bytes -- --include-ignored
```

Set `JMCL_ZIG_SOURCE` and `JMCL_ZIG_ORACLE` to the pinned checkout and its binary.
Integration must review/repin changed routing-source hashes.

## Explicit remaining boundary

Raw OS arguments outside this exact `--file` slice (including raw root directories,
world names, project fields and unrelated command families) are not claimed or
normalized. They retain the previous UTF-8 dispatcher limitation. This commit is
local content filename compatibility, not proof of global native-argument parity.

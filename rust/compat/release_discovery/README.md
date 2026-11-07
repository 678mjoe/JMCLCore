# Frozen release resolver acceptance

Exactly two bounded native scenarios: present adjacent frozen Core resolves with
no explicit path or JMCL_CORE, then real Session handshake/ping/close; after moving
only that owned sidecar, resolution must fail with the precise missing-Core error.
No Tauri, UI, actual release bundle, installer, Java, accounts, archive extraction,
network services, games, or production sidecar changes are covered or performed.

Core source is fixed at bdd6cb3e55d69cb9e0e02037d251635e289207a8, tree
0e8e901bc587cf3c6d7dc1ba78821c4701785e20. Frontend source is fixed at
9c0fd81ce8109c1e35229560fee31308ca114ef5, tree
48ea91981bdb7b375bca8e9bc47738b6c3dde4d1. session.rs and transport.rs are
byte-for-byte copies of that frontend, verified before execution. Main is test-only.

The unique branch/workflow only uses public standard Linux x86_64, macOS x86_64,
macOS aarch64 and Windows x86_64 runners, Rust 1.99.1, locked official-registry
prefetch followed by offline builds. Core uses the native debug profile; only the
resolver probe must use release. Every built/staged binary is checked for actual
ELF64, thin Mach-O64 or PE32+ machine type and hashed. No debug assertion override
is retained, and the probe asserts it is a release/native executable.

Ten private nested levels contain all eight resolver search ancestors. macOS uses
Probe.app/Contents/MacOS; all platforms launch from another private CWD. Windows
path comparison resolves canonical paths and checks samefile; filenames use .exe.
Explicit native --target plus a freshly owned target root prevents stale cross-
target output. Source bytes/hashes are rechecked after both scenarios.

run.py requires --core, --frontend, --runner and a new --work directory. It refuses
optimized Python and dirty/unexpected source checkouts. test_run.py contains pure
fail-closed/environment/architecture/byte-retention guard controls and can also run
under optimized Python. These are not additional Core scenarios.

Each subprocess stdout/stderr combined pipe is captured as bytes, saved as .raw,
SHA256-hashed and emitted as base64 in RAW_COMMAND plus FINAL_REPORT JSON. Decode
base64 to recover original bytes, including CRLF or invalid UTF-8. Actions log
text is only a transport envelope and must not be called raw subprocess output.
Scenario JSON alone is strictly UTF-8 decoded for validation. No artifact uploads
or release publication are involved. Reports assert exactly present then absent.

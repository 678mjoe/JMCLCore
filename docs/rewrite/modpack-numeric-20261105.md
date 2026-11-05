# Modpack CLI numeric grammar, without a file or installation

The workers/retries CLI parser used Rust u8 parsing and rejected Zig-supported
internal underscores. Reusing cli_number::unsigned with checked u8 conversion
fixes this while preserving immediate workers 1..32 / retries 0..5 range checks.

The red case `instance modpack install demo --workers 1__0` produced original
requires --file/status 2 versus Rust InvalidArguments/status 1. Forty-two exact
paired numeric cases now pass. Every request deliberately omits --file: original
code returns before openRoot and Rust returns before stream/provider/transport.
No archive is opened and no installation executes. Owned directories stay empty.
A pure numeric unit test, format check, locked offline build, warning-denied
all-targets Clippy and 22 static audit guard tests (normal/optimized) pass.
Combined with the previous 348 cases, the same Linux executable passes 390
exact status/stdout/stderr comparisons; only two older owned-root cases create
their expected empty roots. Binary SHA256:
6c055bb7d5688315ed3bca059a755af5cc09a19fd8ccc30824f66424d846ab5d.

This is a separate local child of 08c63a2. It does not change the pending remote
publication snapshot or existing CI source. No macOS/Windows result is claimed.

A separate pure numeric differential check compared the actual shared
cli_number::unsigned source against std.fmt.parseInt(u64, ..., 10) compiled with
the pinned Zig 0.16.0 toolchain. All 19,618 strings matched exactly: exhaustive
lengths 0..5 over `01_+-e.` plus unsigned overflow boundaries, long zero strings,
whitespace and non-ASCII digits. This is shared numeric grammar evidence, not
19,618 Core CLI or installation requests. The checked u8 conversion/ranges remain
covered by unit tests and the bounded no-file CLI corpus above.

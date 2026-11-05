# Portable manifest and native group diagnostics integration

This local experimental branch combines the earlier instance/install/launch
usage fixes with both portable server ReadManifest visibility fixes. The latter
were tested separately at remote source 149eebdd6941229fc5622975beab3f5df287afbc
by bounded CI run 42011438956 (Linux 25, macOS ARM64/Intel 23 each, Windows 20).
Those results are not a cross-platform certification of this new integrated tree.

A further pure-input defect was reproduced: `store` followed by a non-UTF-8
unknown subcommand panicked during String argv conversion instead of returning
usage with status 2. The same missing route existed for `server`.
The native diagnostics fallback now handles unknown store/server second arguments.
It always yields for store gc and all 17 known server subcommands. It does not
invoke a handler or inspect operational trailing parameters. ASCII routing is
unchanged. Existing native bridges retain their precedence.

On Linux the final binary passed 184 exact status/stdout/stderr paired cases
against pinned Zig 5a4310debbcee26b3d2391bab2339f79cd6ec52e. This includes 50 group
usage cases (20 ordinary and 30 opaque argv), and the earlier 134 bounded cases.
Only two explicit instance missing-root cases create their owned empty root;
all other cases leave their owned trees unchanged. Four pure diagnostics unit
tests pass, including delegation of every known server/store method.
Locked offline build, format check and warning-denied all-targets Clippy pass.
Binary SHA256: 9630577b752a59946cc7afe18d94a867a3a71ebd186218007a3fdfc655efc450.

No Java/archive, game, network or real service operation was run. New integration
and diagnostics remain local; no existing remote branch or CI pin was modified.

## Invalid version list/show native arguments

A further bounded red comparison reproduced a panic for `version list` followed
by opaque bytes where the original returns usage/status 2. A pure helper now
reuses the existing CLI parser for syntax only. Non-ID opaque parameters are
invalid; an opaque show ID receives a nonempty placeholder solely during syntax
validation. Successful syntax always yields without constructing a transport,
using the placeholder operationally or claiming valid opaque-ID support.
Empty IDs remain empty and malformed repeated options fail immediately.

Twenty-eight additional malformed-only POSIX cases pass with exact output and
unchanged owned directories. Combined corpus now totals 212 cases. Five pure
native diagnostic unit tests pass, including valid opaque show-ID delegation.
Final executable SHA256:
ae7833b446fd4f0305bbdf43e23b827f64848bd3b8bcea89aa6449a9cb374a1d.
Linux-only results; no valid metadata request or network operation executed.

## Invalid plan syntax after opaque loader values

A red comparison also found `install plan x --fabric-loader RAW --unknown`
panicking instead of returning usage. The same pure helper now recognizes plan
syntax. All opaque tokens use a nonempty non-ASCII marker only during validation;
IDs/loaders preserve their nonempty acceptance, while fixed ASCII keys/enums and
numbers reject the marker. Parsed params are always discarded and successful
syntax is never claimed. No valid plan or loader request is executed.

Twenty-six additional malformed plan cases pass; version/plan corpus totals 54,
and the complete bounded comparison totals 238. Six pure diagnostic tests pass,
including all three opaque loader values yielding and flag-shaped loader values.
Final binary: 3c23165f98e5bd60a0d3538aed86f6a949aff8c2ded9c78b1bb050b9c06a6125.

## Nested server usage

Unknown opaque properties/rcon subcommands now emit their original usage without
entering a handler. get/set/command still yield. Twenty additional malformed
nested cases pass, making 258 exact comparisons. Seven pure diagnostic tests
pass. Binary: 7567a2e18b0c30d6fd91b789d3b8d07ce135997ee87167972f0d52368e0380a7.

## RCON numeric grammar (pure validation only)

The CLI's --max-bytes syntax now uses the shared Zig-compatible unsigned parser
and checked usize conversion. It accepts internal underscores and signed zero,
rejects malformed separators/overflow, and preserves the existing final range
check after all options. One pure numeric test covers grammar and usize bounds.
No remote_cli handler, password-file read, socket or actual RCON command was run.
Repeated-option ordering is source-review evidence, not an end-to-end RCON test.

## Content/world nested diagnostic boundary

Nine more nested groups are now explicitly classified: instance mods,
resourcepacks, shaderpacks, datapacks, modpack, worlds, and server worlds, mods,
datapacks. Only unknown third tokens are claimed. Server mods/datapacks retain
Linux usage/status 2 versus non-Linux unsupported/status 1; the non-Linux arm is
source-reviewed and unit-specified, not natively executed in this local work.
Known methods always yield to existing bridges/handlers, and pure tests enumerate
all preserved methods. No content, modpack or world operation is executed.

Final bounded corpus: 348 exact paired comparisons on Linux, including 110
unknown nested cases. Eight pure diagnostic tests and the pure RCON numeric test
pass. Locked offline build, fmt and all-targets warning-denied Clippy pass.
Final executable SHA256:
1f73cf484166fa3d8f319186341a2955d552e5ad1d9083f5e66f7373efbf8f51.

Remaining valid native ID/path/loader flows are deliberately not certified by
usage-only comparisons. Further progress there needs handler-specific parsing
and injected I/O fixtures; no valid request may be inferred safe from this corpus.
Remote CI still pins 149eebd, not this local integrated branch.

Static dispatch audit initially rejected three changed source digests, as
intended. Independent review confirmed unchanged RPC method/handler associations,
then repinned only version.rs, instances/manifest_ordered.rs and cli_native.rs to
source commit 56dfc775ac762410911ca39c5be696472b6de660. All 22 guard tests passed
normally and under Python optimization. Inventory stays 106 names / 105 static
associations; java.runtime.install remains blocked, zero Core requests are
claimed by this audit, and replacement_allowed remains false.

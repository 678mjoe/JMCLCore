# Instance-local CLI native directory and world arguments

The bounded routes are `instance {mods,resourcepacks,shaderpacks,datapacks}
{list,enable,disable,remove}`. Native `--instances-directory` and datapack
`--world` values now retain their OS bytes, alongside the existing mutation
`--file` support. The narrow pre-dispatch hook prevents these arguments from
falling through to `std::env::args()` and panicking on non-UTF8 arguments.
The production change is confined to `content/cli.rs`.

## Source contract

Pinned Zig `5a4310d`, `src/cli/content.zig` (`runContentList`,
`runContentSetEnabled`, `runContentRemove`) validates ID, parses options in
order, checks selector cardinality, opens the instances root, loads the
manifest, then opens the game directory. `src/cli/datapacks.zig` instead joins
the game path directly without loading an instance manifest. This asymmetry
is intentional compatibility, including missing-root creation effects.
Repeated options retain the last value. Empty directory is syntactically
accepted only by ordinary content list; other local routes reject it.

Directory values use the existing checked native-path converter and native
root helpers; ID remains validated text and path joining does not use lossy
conversion. World bytes use the existing byte-world helper. Saves lookup
still precedes world-name validation; mutations retain the same
`session.lock` check before registry/selector operations. Selected existing
world aliases retain existing semantics, not a new blanket symlink policy.
No filesystem flags, operational manifest parser, server route, public API,
install/adopt/set-version path, or Java entrypoint changed.

## Reproducible bounded evidence

```sh
python rust/compat/content_cli_native_paths.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/checkout-at-5a4310d
JMCL_ZIG_ORACLE=/path/to/pinned/jmcl-core \
  cargo test --locked --test content_cli_os_bytes -- --include-ignored --nocapture
```

On Linux, the standalone corpus passes 400 exact command comparisons:
stdout/stderr bytes, exit status, complete relative disk contents/modes/link
counts, Unicode/raw arguments, missing/malformed manifests, true missing
saves, missing worlds, argument conflicts, repeated/empty options, owned
aliases and a held owned world lock. Positive disable → enable → remove
sequences cover all four content families with inert files. Readonly hardlink
peer/sibling bytes, permissions, mtimes and inode identity are independently
preserved, with link-count reduction only on successful removal. List and
argument-error cases assert no writes; missing-root cases assert precisely
the source-created directories. The earlier binary fails the positive
raw-directory list control with exit 101, rather than an equal-error pass.

The native Rust test also checks raw native arguments reach argument
validation before filesystem access, including Windows unpaired-surrogate
arguments. Existing raw-file differential controls stay active; the server
raw-directory route remains outside this hook extension.

Native macOS/Windows execution remains a separate CI gate. APFS may reject
physical raw names with EILSEQ: the script then compares eight exact rejected
raw-directory commands and runs Unicode controls, without claiming raw-name
success. The Python Windows corpus uses physical lone-surrogate WCHAR directory,
world and file arguments, with explicit WTF-8 snapshot/output encoding.
That native execution remains pending; only ERROR_INVALID_NAME is treated as
an unavailable physical-name capability, followed by exact rejection checks.
Windows does not claim Unix opaque-byte, owned-symlink or held-flock coverage.
All owned Windows readonly fixtures are retained rather than chmod-cleaned;
missing-saves fixtures are constructed absent without deleting readonly links.
No readonly mismatch is accepted or normalized. This is local content
coverage, not acquisition, archive, game execution or complete CLI parity.

### Failure diagnostics

Each named case collects both engines before enforcing strict comparison.
A positive-contract failure stops further mutations for that engine, retains
its owned fixture, and records each completed invocation's exact stdout,
stderr, status and raw before/after tree (including peer inode, mtime, mode
and link count). The other engine still runs its independent fixture.
`LOCAL_CLI_CASE_FAILURE` / `LOCAL_CLI_REJECTION_FAILURE` report both sides;
failures remain failures, with no alternate expected outcome. A Linux
negative harness control put the old panicking binary first: its exit-101
record and retained tree were captured while the second binary completed
all four positive commands, and the test still failed as required.

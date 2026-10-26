# Existing-file servers-root fidelity

Pinned Zig `src/server/server_store.zig::openRoot` delegates to
`createDirPathOpen` with directory iteration and `follow_symlinks = false`.
Zig 0.16 `std/Io/Threaded.zig::dirCreateDirPathOpenPosix` opens first, and only
creates the path after `FileNotFound`. An existing regular file therefore fails
with `NotDir` from the directory open.

Rust's preliminary `create_dir_all` instead returned `AlreadyExists`, which
prevented its existing nofollow directory open from running. The helper now
allows **only** that error to proceed to the existing open. Other errors still
return unchanged, and `RDONLY | DIRECTORY | NOFOLLOW | CLOEXEC` flags are
unchanged. This is not a blanket error-name conversion or symlink-policy change.

The show CLI's existing same-call diagnostic slot also retains root-open error
detail. RPC envelopes are unchanged; no root/manifest is re-opened to guess a
message. Delete already captures the phase diagnostic in its separate fix.

## Evidence

`server_root_open.py` compares **40 exact show/delete RPC/CLI cases** on Linux:
normal, missing/nested root, existing root file, ancestor file, target file,
empty root, final-root alias, allowed ancestor alias, dangling-root alias and
target alias. Normal deletion is asserted as successful; invalid roots/targets
remain untouched. Missing roots create exactly their expected directories.
Sibling and peer bytes/modes/mtime/inodes remain unchanged, with the expected
single link-count decrement only when the owned target is deleted. No process
state or executable exists and no status/start/stop command runs.

The root-file red case was directly observed on both CLI paths: source says
`failed to open servers directory: NotDir`; old Rust show discarded the detail,
and old Rust delete reported `PathAlreadyExists`. Target-file `NotDir` already
matched and is retained as a control. The new corpus passes, along with the
focused root-open unit test, prior 56 no-state deletion comparisons, 75 show/get
comparisons and 458 manifest read-view comparisons. All-target Clippy passes.
`lifecycle.rs` and all its process/guard/signal logic remain byte-identical.

Native macOS/Windows execution remains pending. Server management remains
Linux-only: the portable script requires unsupported-platform responses and
complete disk preservation elsewhere. Windows does not assume symlink creation
privileges; those fixture cases are explicitly excluded there, not counted as
filesystem-behavior validation.

```sh
python rust/compat/server_root_open.py \
  --zig /path/to/pinned/jmcl-core --rust rust/target/debug/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
cargo test --locked --lib root_open_preserves_directory_errors_and_nofollow
```

# Linux candidate follow-up, 2026-11-07

This records **new execution after** the documentation audit in
[the acceptance map](candidate-acceptance-20261107.md). It does not rewrite the
November 6 native Session evidence or turn the original static audit into a
runtime pass. Rust replacement is still not approved.

A later, separately scoped [server lifecycle execution record](candidate-server-followup-20261107.md)
adds installed-JVM property probes and owned synthetic server execution. It does
not change the no-real-Java scope of the earlier results described here.

The separately reviewed [client-world follow-up](worlds-candidate-20261107.md)
adds owned synthetic NBT/region/world-ZIP workflows and pinned CLI comparisons.
It does not claim Java runtime archive installation or real game generation.

## Exact tested subject

- Preserved remote commit: `d8bb78d3eca19cd068adaff07920522ae1c2e593`.
- Executed local commit: `25f910409581e42c7bc5792a3b943d8e086e5924`.
- Identical tree: `177db65cdf81ded3cb8202f3c6dc686ba81eaf84`.
- Platform: Linux x86_64 GNU; existing pinned Rust 1.99.1. Zig 0.16.0 is
  used only by the pure URI parse/format fixture.
- Rebuilt Core SHA-256:
  `dad858d76f05cc4fad65814b03790a83e02e9244c5fa1d3155f882b89c1fd1f5`,
  also matching the earlier retained executable hash.

All Cargo commands used `--offline --locked`, default features only, an owned
target directory and the existing dependency cache. No dependency download was
needed. The follow-up changes only documentation and retained evidence, not
production sources, Cargo manifests/locks or CI.

## Executed checks and scope

| Check | Observed result | Boundary |
|---|---|---|
| `cargo test --manifest-path rust/Cargo.toml --offline --locked --all-targets --no-run` | Passed; test executables compiled/linked. | Does not run the full test suite. |
| `cargo clippy --manifest-path rust/Cargo.toml --offline --locked --all-targets -- -D warnings` | Passed. | Main Rust crate only, not the separately imported frontend harness. |
| Two exact `url_construction_contract` tests | Each 1 passed / 0 failed / 0 ignored. | Same 32 records: Rust construction and pinned Zig std.Uri/Rust pairs. No HTTP Client, DNS, request, TLS or redirect. |
| Six exact local-state tests | 6 passed, none failed/ignored. | Actual local account/instance/content/store APIs in owned temporary directories. No Core process. |
| Four exact native/stopped-server workflows | 4 passed, none failed/ignored. | One launch executor with injected metadata/resolver, one direct process API, both using a benign compiled Rust child; two real Core RPC stopped-server workflows. |
| Four exact synthetic authentication workflows | 4 passed, none failed/ignored. | Production auth algorithms with a queued in-memory transport and synthetic tokens. No account, HTTP client or real service. |
| `cargo test --manifest-path rust/Cargo.toml --offline --locked --test gui_launch_plan -- --nocapture` | 8 passed, none failed/ignored. | Pure metadata/plan/EventWriter construction, not a GUI, RPC or subprocess. |

These are finite tests of different layers, not a unique-requirement total or a
compatibility percentage. The exact test names, commands, toolchain and log
hashes are retained in the [manifest](../../rust/compat/evidence/candidate-followup-20261107/manifest.json).

### Local state and recovery

Run each with the common prefix
`cargo test --manifest-path rust/Cargo.toml --offline --locked`, the selector
below, and `-- --exact --nocapture`:

- `--lib accounts::tests::save_load_list_delete_public_metadata`: save, load,
  update, list, serialized public metadata and delete; all values are synthetic.
- `--lib instances::unix::tests::lifecycle_preserves_layout`: create two
  instances, sorted listing, private game directories, uninstalled state and
  deletion. No validating metadata request or installation.
- `--test content_adopt adoption_reuses_real_file_identity_and_manual_registry`:
  adopt ordinary bytes, repeat adoption, verify read-only CAS and actual inode
  sharing, and preserve unrelated content.
- `--lib content::tests::toggle_remove_preserve_shared_inode`: disable,
  repeated disable, remove and preserve the original shared object's bytes.
- `--lib content::tests::missing_files_and_partial_toggles_are_recoverable`:
  reconcile registry state with a missing/reappearing ordinary file, then remove.
- `--test store_root_errors existing_and_missing_owned_root_creation_stays_exact`:
  create/reopen CAS and artifact roots without extra directory contents.

### Native launch and stopped-server RPC

The same exact-test prefix/suffix selects:

- `--test launch_executor native_launch_and_cleanup_on_all_owned_error_paths`:
  injected metadata and resolver, temporary native-directory preparation, benign
  native execution, log redaction, exit 7, missing-child/error/drop cleanup.
  `install::prepare` receives an empty native-extraction list; no archive is
  read or extracted. `JavaDownloadFailed` is an injected error value, not an
  attempted download. Java discovery is never called.
- `--test launch_process native_child_streams_literal_args_and_environment`:
  compile the reviewed `tests/fixtures/launch/child.rs` with local rustc, then
  check real process events, binary/empty-line output, exit, literal argv and
  filtered synthetic environment. No JVM is involved.
- `--test server_lifecycle offline_properties_and_unknown_parameters`: real
  Core RPC properties get/set, secret-field redaction, stopped status and stop
  error. The fixture's Java-named shell file is not executed or probed.
- `--test server_lifecycle server_worlds_keep_base_dimensions_and_properties_coupled`:
  real RPC list/rename/delete for plain owned directories, coupled dimensions
  and `level-name`, while preserving an unrelated directory. No backup/archive.

These are not the unchanged frontend Session/Tauri launch chain. The standalone
executor test uses an injected resolver; the process test calls `process::run`
directly without a resolver. The stopped-server tests use the actual Rust RPC
dispatcher.

### Authentication and launch construction

The exact `--lib` authentication names are `auth::tests::full_exchange`,
`auth::tests::successful_refresh_exchange`,
`auth::tests::refresh_rotation_failure_is_a_result` and
`auth::tests::device_begin_and_poll`. They cover exchange, refresh-token rotation
on success and later-stage failure, and pending/slow-down/authenticated polling,
with queued synthetic responses only.

The eight `gui_launch_plan` tests cover modern/legacy arguments, GUI options,
rule selection, target classpath delimiters, secret placeholders, forced/manual
runtime construction and sticky EventWriter failure. Fictional Windows/macOS
targets inside a pure plan test are not native platform execution.

### Accepted URL differences remain accepted

The two exact names are
`accepted_url_deviation_has_exact_named_construction_results` and
`pinned_source_url_construction_matches_exact_bytes`. The latter is explicitly
selected with `-- --exact --ignored --nocapture` and
`JMCL_ZIG_COMPILER` pointing to existing Zig 0.16.0; its test is executed, not
silently counted as an ignored pass. The former uses `-- --exact --nocapture`.

Both use `--test url_construction_contract`. They assert the same 32 records:
six [COMPAT-URL-001](../../rust/compat/url-compatibility-decision.md) differences,
eight strict target controls and eighteen construction-only diagnostics.
Those accepted differences are not wire parity or a requirement to add raw-byte
URL transport. Unrelated parser, network, TLS and security requirements remain.

## Deliberately unexecuted scope

- No full test suite, `--all-features`, runtime acquisition, real JVM/game,
  archive parsing/installation, real account/provider, or Tauri/UI operation.
- No full server start/restart fixture. Source review found that its explicit
  Java fixture still goes through discovery of JAVA_HOME, PATH and Linux's
  fixed JVM directories. Clearing environment variables alone does not isolate
  discovery. This was excluded from the current **no-real-Java test slice**;
  it was not a tool denial or a claim that all Java probing is prohibited.
  Further testing needs an explicit environment/control plan and must preserve
  the separate Java-acquisition/archive restrictions.
- World backup/restore and loopback RCON/Query tests were not selected. The
  former processes archives; the latter uses real local sockets and also enters
  server-start discovery. Their fixture source is not a passing result.
- No Windows or macOS execution. Current native evidence remains Linux-only.

There were no permission denials in this follow-up. The next native matrix can
use platform-appropriate selections from the reviewed allowlist, compile-only
and Clippy commands on authorized Windows/macOS executors. `server_lifecycle`
is Linux-only, and `instances::unix` does not apply to Windows. Check each
target's actual inventory and require nonzero intended execution; a filtered
zero-test result is not a pass. Do not substitute the broad existing workflows,
whose runtime scope includes tests intentionally excluded here.

## Verbatim logs

Files are stored under
[`rust/compat/evidence/candidate-followup-20261107`](../../rust/compat/evidence/candidate-followup-20261107).
Compiler/test output is preserved verbatim, including local build paths.
The three exact-test runner logs also record each command as a JSON array.

| File | SHA-256 |
|---|---|
| `compile.log` | `4ccc29fcceeca751ea084c82ea45df21a0859705dff63de79fe9ff39d0b952b8` |
| `clippy.log` | `91299f49e4daabf5031d21e8a4072c563e1fedb3466075e1624eebf6e0500d2d` |
| `url-rust.log` | `14f5f991833b5714f9afaedd4459ee2e64a9666f20f3daa07ebb9590f5493645` |
| `url-paired.log` | `96da935fac2bc301b66c32619b94fc326f501a6ce3b6c50ac4b5ca4c11706202` |
| `local-state.log` | `1b9b24a2b0999a8f91c02d4c30895cd76441712dc79806d658f0de94f9bd9ebb` |
| `native-workflows.log` | `21bd3bab50333c83a828ea6d03d93fc7625db09a1d1b22150c9bffb8dabdcefb` |
| `auth-synthetic.log` | `7f6e1218889f3c66e8f2b21f060fced776407bb4b9cc00c423432be552f56aef` |
| `gui-plan.log` | `1d297ef7aa378575920038d9d1d8d74a2a9a039fe9afb8fca9bf1212727e123d` |

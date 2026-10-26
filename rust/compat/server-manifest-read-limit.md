# Server manifest read-limit diagnostic

`servers/store.rs` now maps only the shared bounded reader's exact
`InvalidData("TooLarge")` sentinel to `ServerManifestTooLarge` when reading
`server.json`. Both operational `load` and display `load_read` use this private
wrapper. The original 64 KiB bound, max+1 read, open flags, traversal and all other
errors are unchanged. `servers/lifecycle.rs` is byte-identical; this is neither a
CLI string normalization nor a global error rewrite.

Pinned source 5a4310debbcee26b3d2391bab2339f79cd6ec52e:
`src/server/server_store.zig` load lines 112–118 maps StreamTooLong and oversized
bytes to ServerManifestTooLarge before parsing. Rust previously surfaced the
reader's generic sentinel before either manifest parser could apply its own limit.

Baseline ordered-parser executable c94a5f6d1022b36cef64e031017a98e5d31e19a7820398d0f4a8c9dd41c2b9e5
has six diagnostic differences in 32 owned read-only pairs. The fix yields zero
differences and zero snapshot/contract failures. Both show formats, RPC get/list,
65,535/65,536/65,537/131,072-byte inputs, malformed enum before/above limit,
missing files and invalid request-ID precedence are covered. Invalid show ID
returns source usage/code 2 (not generic InvalidArguments); the initial fixture
expectation was corrected after both engines agreed on that result.

Exactly these two named Rust units executed, one test each, and passed:
- `servers::store::manifest_read_limit_tests::owned_manifest_read_limit_names_do_not_change_other_errors`
- `instances::manifest_ordered::size_tests::existing_four_manifest_size_guards_precede_schema_parsing`

The first uses only direct metadata load/read with owned files and hardlink peers,
including unchanged missing-file, NotFile and InvalidServerId controls. The second
checks all four direct parser size boundaries. The prior 73 strict ordering pairs
remain green, and the formerly separate oversized-server diagnostic probe now
passes. Clippy all-targets and diff checks pass. Final executable SHA256:
6785f4acd7e36abe524d7326c9ab500659dfa967325bb3c77e09c2e41c32afe8.

```
python rust/compat/server_manifest_read_limit.py --zig <pinned-core> \
  --rust <rust-core> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Every executed core command is show/get/list on synthetic owned metadata, with
readonly hardlink peers and full raw before/after snapshots. No process state,
PID, lifecycle operation, mutation, installation, Java, archive or network call.
Non-Linux runs 32 unchanged platform gates with no metadata creation/access;
native acceptance remains separate CI evidence. Other consumers inherit only
this exact oversized-manifest error spelling, but remote/installation/mutation
consumers were source-audited, not executed for this slice.

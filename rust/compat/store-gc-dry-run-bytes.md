# Explicit owned-store dry-run RPC fixture

`store_gc_dry_run_bytes.py` is a bounded operational acceptance fixture, separate
from the pure parameter tests. It sends only `store.gc` RPC requests with literal
`dry_run:true`, a nonempty absolute preexisting owned store root, and an explicit
age of 0 or 86400. It never invokes the CLI, omits/defaults the root, sends false,
or requests deletion. No Java, archive, provider, authentication or network
operation is part of the fixture.

## Reviewed safety boundary

The pinned Zig collector and both Rust platform collectors return from the
true dry-run branch before digest locks or deletion. Existing-root opens and
object directory opens do not create paths. The script binds reviewed normalized
source hashes and requires exact source HEADs and executable SHA256 values.
Every Core subprocess rechecks its binary hash and validates the outgoing
method, literal boolean, complete params and decoded path against the owned root.
Guard-only negative cases are never forwarded.

The owned tree contains eligible, hardlinked and future-dated opaque files plus
an ignored filename. A peer hardlink outside the store but inside the owned
workspace, and a default-root canary, are included in every snapshot. Exact
summary and raw response equality are required. Before/after snapshots cover
names, types, content hashes, sizes, modes/readonly attributes, modification and
change timestamps, identities and link counts. Access time is excluded because
reads can update it. Added paths, including locks, fail the comparison.

Physical opaque-name cases run only when supported. Filesystem EILSEQ exclusions
and unrepresentable Windows FF paths are explicitly reported, not counted as
raw-name success. Native conversion alone is not filesystem acceptance.

Fixtures use `mkdtemp` and are deliberately retained on every outcome/platform;
the path is printed immediately. No post-RPC cleanup changes shared readonly
attributes or removes failed diagnostic evidence.

## Exact invocation

Supply independently verified executable hashes and source commits; do not infer
binary provenance merely from a checkout:

```sh
python rust/compat/store_gc_dry_run_bytes.py \
  --zig /absolute/pinned-core \
  --zig-sha256 VERIFIED_ZIG_SHA256 \
  --zig-source /absolute/pinned-source \
  --zig-head 5a4310debbcee26b3d2391bab2339f79cd6ec52e \
  --rust /absolute/exact-combined-core \
  --rust-sha256 VERIFIED_RUST_SHA256 \
  --rust-source /absolute/exact-combined-source \
  --rust-head VERIFIED_RUST_COMMIT
```

The script itself verifies the approved source-text hashes before operation;
a meaningful source change requires review before updating these pins. No
failure is retried or normalized away. The maximum supported matrix is 28
paired outcomes; capability exclusions reduce it explicitly.

## Verified Linux execution

The independently reviewed fixture passed all **28 paired outcomes** on exact
Rust commit `b4dbe78020c81615a820874bd57f636550f52398` (tree
`af05e94ec12b024199dfb1c31c15afdbfc2b8c7b`), executable SHA256
`3aa1de349bd2dcc088bf652178687a990e7781b97b059abce191074d871d7966`,
against pinned Zig `5a4310debbcee26b3d2391bab2339f79cd6ec52e` (executable
`0d49ff4130720c67397c3f673afaaf226f4b1f8384f68ea599e43340030c164b`).
Raw responses matched and every workspace snapshot was unchanged; no locks,
deletions or added paths occurred. Both physical opaque-name forms were supported
on this Linux filesystem. This proves only the guarded existing-root dry-run
matrix, not default-root behavior, deletion mode or other native filesystems.
Windows/macOS and exact publication checks remain pending.

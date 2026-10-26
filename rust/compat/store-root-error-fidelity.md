# Existing CAS root error fidelity (not COMPAT-CAS-001)

The user-approved narrow change handles only `create_dir_all`'s `AlreadyExists`
result by continuing to the existing directory open. Unix retains the same
DIRECTORY/NOFOLLOW flags; Windows retains its existing native directory open and
reparse checks. All other creation errors return unchanged. No CLI error text is
normalized, no metadata guess substitutes for opening, and no lock layout/policy
changes. Store::open and ArtifactRoot::open (and parent helpers using walk) share
this helper; no installer, provider, Java or archive caller was executed.

Pinned Zig artifact_store.openRoot uses createDirPathOpen with follow_symlinks=false.
Its POSIX backend opens first and creates only on FileNotFound; an existing file
therefore yields NotDir. Rust previously stopped at std's AlreadyExists before
its directory open, exposing PathAlreadyExists to the CLI. The change allows the
same final validated handle to classify the actual object. If a concurrent actor
changes it, successful access still requires the unchanged no-follow/reparse
checks; this source argument is not a new race experiment. Existing ancestor-alias
policy and creation of missing owned directories are unchanged.

Base: CLI consumer stage 05dfec2ce1b5a3f6053c20d80ab7dbc68fb6a035.
The direct regular-file root test was red (AlreadyExists rather than NotADirectory)
and is now green for both public root APIs. Tests also cover existing/missing
roots, a regular-file parent, final directory aliases, Unix final file aliases,
and an allowed owned ancestor alias. All sentinels are owned temp files; no
external/real paths or shared immutable attributes are modified.

The 8 previously separated blocked-store CLI sequences / 16 CLI pairs now match
source exactly with unchanged raw snapshots. They are restored to the full
adoption CLI corpus: all 150 sequences / 600 total calls match, with zero direct
state failures. Its 12 CAS lock-layout differences remain separately visible and
asserted under COMPAT-CAS-001; that accepted difference never covered root errors.
The earlier name-stage scope note records its historical limitation; this stage
closes those 8 diagnostic cases rather than hiding them behind an allowance.

```
cargo test --test store_root_errors
python rust/compat/store_root_cli_errors.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source>
```

Local results are Linux-only. Windows direct-root tests include strict owned
junction creation/control checks and have no generic error allowance; native
execution remains a required separate gate. Server CLI diagnostics are Linux-only.

Additional deterministic control: an owned path containing NUL returns the same
non-AlreadyExists error kind/raw OS code as the original creation call and leaves
its fixture empty. Existing-file identity checks use device/inode on Unix and
handle-derived volume/file-index on Windows (Windows execution still pending).
All 4 direct Linux root tests and Clippy pass. Prior server adoption target corpus
remains 320 exact calls with its 25 separate accepted lock-layout differences.

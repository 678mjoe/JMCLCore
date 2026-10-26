# Server world read metadata

The private byte-preserving world read view is used at four read-only call
sites: Linux `server.worlds.list/get` and portable `server worlds list/show`.
The latter CLI branch is used on Linux too. Request, manifest and properties
parsers, fixed property-derived world order, platform gates and icon error
handling are unchanged. Shared operational world and mutation/archive branches
are untouched.

Source review: pinned Zig `5a4310de`, `src/protocol/worlds.zig`
`openServerWorldContext` / `writeServerWorldsListResult` /
`writeServerWorldsGetResult`, and `src/cli/worlds.zig` `openServerDir` /
`runServerList` / `runServerShow`. These read routes open the root, validate the
manifest and open the server directory before world-name lookup. List derives
base/Nether/End names from properties, preserving that order; it does not sort
or enumerate arbitrary directories. No stopped-server guard or process state
lookup is used. Optional world `session.lock` probing is independent of process
identity; these fixtures omit that file.

```sh
python rust/compat/server_world_metadata.py --zig /path/to/zig-core \
  --rust /path/to/rust-core --oracle-source /path/to/pinned/source
```

Linux: 138 strict paired commands pass: 90 metadata commands and 48 order/error
controls. Metadata covers Unicode/control/opaque bytes, wrong tags, empty/null
and duplicates. Controls cover three-world order, absent/malformed properties,
a non-directory sibling, missing/invalid world names, malformed manifests and
icon-read errors (null payload while `has_icon` stays true). CLI text and JSON
are exact; RPC explicitly asserts the non-Linux unsupported gate when run
there. Whole owned fixture bytes, permissions, mtimes, inode/link identities
and readonly hardlinked peers remain unchanged. There are no PID/process-state
files, executables, cleanup candidates or external writes. Only these read
methods run, not status/start/stop, archives, Java or network operations.

The separate portable Unix root followup now lets only `AlreadyExists` from
`create_dir_all` continue to the identical no-follow directory open. This
matches the pinned `server_store.openRoot` createDirPathOpen classification:
a regular-file root reports `NotDir`. All other creation errors, final-alias
flags and the Windows delegate are unchanged. Central Windows `store::walk`
has its own already-integrated matching fix; this commit does not edit it.

`server_world_root.py` takes the same three command arguments and exercises
only list/show: existing/missing root, root file, file ancestor, empty path and
Unix final/ancestor aliases, text/JSON. Linux 28 exact commands pass, with only
the specified missing-root directory creation allowed; preexisting objects
and readonly peers are preserved. A direct Unix root-only unit additionally
checks NUL rejection without passing an impossible NUL argv to a process.
The previous four CLI root-file discrepancies now compare strictly equal.
Shared non-read callers retain the same helper but are not executed here.
Native macOS/Windows validation remains pending exact-tree CI.

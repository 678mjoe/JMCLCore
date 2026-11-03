# Top-level native argv diagnostics

Implementation checkpoint: `2783d8a0e5f8dfd47b6bebeefcbddeaee9cfc9a8`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

The old executable panicked for `help` plus an opaque argument instead of printing
help. A retained red comparison establishes that defect. The new pure bridge
runs after existing native bridges and before String argv conversion. It handles
help/--help, rpc with extra arguments, version except list/show, and unknown
primary command bytes. All operational commands and hidden server commands are
left to existing routes. This is not complete CLI byte compatibility.

Linux evidence: 30 exact status/stdout/stderr comparisons passed with no file
writes; 3 pure unit tests passed; offline locked build, all-target Clippy and
formatting passed. A premature test attempt against the still-old executable
also failed, then the completed build passed. No result from that attempt was
counted. Static regression tests passed normally and optimized (22 each).

No Core service, Java, archive, provider, network or real user data operation is
part of these diagnostics or corpus. Windows fixture definitions exist but no
native Windows execution occurred. No remote branch was modified.

Reproduce: `python rust/compat/cli-diagnostics-byte-argv.py --zig PATH --rust PATH`.

# Pure verification URI predicate corpus

Run prepare.py OUTPUT_DIRECTORY. Compile its candidate.rs with rustc edition2024
and oracle.zig with pinned Zig0.16 as candidate/oracle in that directory. Run
compare.py there. No Core process, authentication, network or file fixture is used.

The Rust adapter extracts the checkout's https function and cli_number parser,
replacing only the Zeroizing Vec wrapper with Vec. No decision logic changes.
The Zig adapter copies the two pinned pure URI predicates, using std.Uri and
HostName.validate. It is not an end-to-end authentication test.

Seed20261105, 7,590 combinations and10,000 random URI strings:17,590 cases,
zero mismatches after the port-spelling and percent-decode-buffer patches.
This finite corpus is not proof of complete compatibility.

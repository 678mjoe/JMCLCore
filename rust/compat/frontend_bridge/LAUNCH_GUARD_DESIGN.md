# Launch isolation specification and review boundaries

This specification follows the historical blocked result in `LAUNCH_CHAIN.md`.
The original design and blocked evidence are preserved in commit `21c2e41`.
Independent review made the descriptor-import denials and single-threaded
installation requirements below mandatory. A separate `launch_network_guard.py`
candidate follows this specification; its review and execution evidence must be
reported separately. Retain the original guard and existing CI lanes.

## Minimal candidate change

The recorded Rust 1.99.1 GNU standard-library fork/pre-exec path uses an
AF_UNIX socketpair for the spawn error channel. Its parent calls libc `recv`
with flags zero, which maps to Linux `recvfrom(fd, buffer, length, 0, NULL,
NULL)`. A successful exec closes the child error-channel endpoint; a failed
exec writes an eight-byte errno/marker reply.

A **separate reviewed opt-in guard** could allow only this recvfrom shape:

- retain x86_64 LP64/audit-architecture checks and reject all x32 syscalls;
- close all inherited descriptors above 2, reject socket descriptors at 0–2,
  and install before starting the test, Core or native fixture;
- require a fresh, controlled, single-threaded runner before FD cleanup and
  again before installation. `seccomp(..., flags=0, ...)` does not TSYNC existing
  sibling threads; refuse them rather than leaving an unfiltered sibling;
- keep `socket()` always denied, including AF_UNIX; permit only
  `socketpair(AF_UNIX, ...)`, never AF_INET/AF_INET6/AF_PACKET pairs;
- keep connect, bind, listen, accept/accept4 and all io_uring calls denied;
- allow recvfrom only with flags 0 and both source-address pointers NULL;
  compare full pointer widths, not just low words; deny other shapes;
- leave sendto, sendmsg, recvmsg, sendmmsg and recvmmsg denied unless separate
  source/runtime evidence establishes a necessary and safe narrower exception.

Classic seccomp BPF cannot inspect the socket family behind an arbitrary fd.
The safety argument therefore depends on **no network socket existing in the
entire guarded process tree**, not on a claim that recvfrom itself is local.
Socket creation/import and initial descriptor closure are part of that
invariant. Existing plain read/write would also transfer data on an imported
network socket, so preserving this invariant already matters for the original
guard.

## Is sendto needed for exec failure?

Not in the statically inspected tested Core binary. `Command::spawn` calls
`Socket::write` at `0x1001862` on failed exec; that function directly calls libc
`write` at `0x1002737`. It does not call send/sendto for the eight-byte error
reply. Thus the minimal current proposal does **not** allow sendto.

Do not extrapolate this to every Rust/libc/platform version. A future toolchain
that requires sendto would need a new evidence-based review. A NULL-destination,
zero-address-length sendto exception could only be justified by the same
no-network-fd invariant and verified flags, not by assuming all connected
sockets are local. Keep it denied without that evidence.

## Descriptor import and hostile-code boundary

- SCM_RIGHTS can import file descriptors through recvmsg. Keep recvmsg and
  sendmsg denied, including their multi-message variants. `recvfrom` does not
  expose ancillary descriptors. No outside process holds an endpoint of a
  newly created private socketpair unless a separate import/export path exists.
- Standard-library pidfd support has sendmsg/recvmsg branches. This proposal
  does not enable them. The current native Session handshake's posix_spawn path
  worked under the original guard. A future fallback requiring pidfd exchange
  must fail closed rather than silently gain descriptor-passing permission.
- Mandatorily deny `pidfd_getfd` in this candidate: otherwise a process
  with sufficient ptrace-style rights could import an outside process's socket.
  `pidfd_open` alone supplies a process handle, not a network socket; nevertheless
  it does not justify permitting getfd.
- Mandatorily deny ptrace and process_vm_writev in this candidate to prevent obvious
  code-injection routes into an unguarded process. Do not rely on this cloud
  environment's present PTRACE_TRACEME denial as the guard's security property.
- Keep all io_uring setup/enter/register blocked, preventing asynchronous network
  operations and use of registered descriptors to bypass syscall filtering.
- Path access, arbitrary devices, writable executables, outside-process
  cooperation, hostile privileged code and every conceivable kernel fd-import
  route are not covered by this narrow analysis. `/proc/<pid>/fd` is not a
  general way to reopen a socket, but that fact is not a proof of general
  filesystem confinement. Trusted test sources, owned fixtures and no external
  helpers remain mandatory. A hostile-code sandbox would need a separate
  namespace/filesystem/privilege threat model and review.

## Exec inheritance and mandatory acceptance checks

Install `no_new_privs` and the filter in a fresh, controlled, single-threaded
runner child before exec. Refuse any pre-existing sibling threads. Filters
must remain inherited by the test's threads, Core, the probe child and the
launched child; no alternative unguarded launcher or subprocess retry is
permitted. Runtime artifacts and metadata must still be owned synthetic files,
with forced absolute native-child override/local policy. This proposal does
not authorize Java/archive/network execution.

Before accepting a new guard:

1. Inspect the proposed BPF, architecture checks and descriptor cleanup. Preserve
   unchanged existing-guard tests independently.
2. Verify forbidden socket families, socket/connect/bind/accept, SCM_RIGHTS
   syscalls, io_uring, pidfd_getfd, ptrace and process_vm_writev fail closed.
   Test rejection of existing sibling threads and inheritance by newly created
   threads. Reject bare syscall numbers 512–547 as well as the x32 bit, covering
   the historical pre-5.4 ABI ambiguity. Test rejected recvfrom
   argument shapes as well as the exact allowed local pair read.
3. Verify inherited socket stdio rejection, extra socket-fd closure, and the
   no-network-fd invariant after at least two exec boundaries. Use only local
   AF_UNIX fixtures to test cleanup; no external connection is needed.
4. Execute the retained benign missing/successful pre_exec reproducer under
   the new guard and require normal spawn-error/exit behavior. This is the first
   runtime gate; failure stops before Core. Do not replace failure with a mock.
5. Run the exact opt-in launch chain and require all assertions plus an original
   Session trace. Confirm no runtime acquisition branch was entered and no
   unintended test ran. Repeat failed-prepare/retry, nonzero exit, cleanup and
   reopen coverage. A build-only result or partially reached chain is not green.

Review recommendation: the smallest candidate for this toolchain is the exact
NULL-address/flags-zero recvfrom exception plus explicit descriptor-import
defenses. It is narrower than allowing general local socket send/receive, but
still requires separate approval, implementation review and fresh evidence.

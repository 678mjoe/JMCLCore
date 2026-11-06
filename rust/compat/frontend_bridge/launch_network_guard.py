#!/usr/bin/env python3
"""Launch-only trusted-fixture network denial; never changes system settings.

Separate from the unchanged historical network_guard.py. A fresh single-threaded
runner owns only nonsocket stdio at installation. New threads/exec descendants
inherit the filter. This is not filesystem or hostile-code confinement.
"""
import ctypes
import errno
import os
import platform
import resource
import signal
import socket
import stat
import subprocess
import sys
import threading
import time

ALLOW = 0x7FFF0000
KILL = 0x80000000
AUDIT_ARCH_X86_64 = 0xC000003E


class Filter(ctypes.Structure):
    _fields_ = [("code", ctypes.c_ushort), ("jt", ctypes.c_ubyte),
                ("jf", ctypes.c_ubyte), ("k", ctypes.c_uint)]


class Program(ctypes.Structure):
    _fields_ = [("len", ctypes.c_ushort), ("filter", ctypes.POINTER(Filter))]


def platform_check():
    if sys.platform != "linux" or platform.machine() != "x86_64" or ctypes.sizeof(ctypes.c_void_p) != 8:
        raise RuntimeError("Launch guard supports only Linux x86_64 LP64")

# Only the exact NULL-address/flags-zero recvfrom form is excepted below.
# AF_UNIX pair creation is the only permitted source of new socket descriptors.
DENIED = {
    "socket": 41, "connect": 42, "accept": 43, "sendto": 44,
    "sendmsg": 46, "recvmsg": 47, "shutdown": 48, "bind": 49, "listen": 50,
    "accept4": 288, "recvmmsg": 299, "sendmmsg": 307,
    "io_uring_setup": 425, "io_uring_enter": 426, "io_uring_register": 427,
    "ptrace": 101, "process_vm_writev": 311, "pidfd_getfd": 438,
}


def single_thread_check():
    # flags=0 does not TSYNC existing siblings. Refuse them before touching FDs.
    if os.listdir("/proc/self/task") != [str(os.getpid())]:
        raise RuntimeError("Launch guard requires a fresh single-threaded runner")


def filter_rows(expected_arch=AUDIT_ARCH_X86_64):
    rows = [(0x20, 0, 0, 4), (0x15, 1, 0, expected_arch), (0x06, 0, 0, KILL),
            (0x20, 0, 0, 0), (0x45, 0, 1, 0x40000000), (0x06, 0, 0, KILL)]
    # Also close the historical pre-5.4 bare-number x32 ambiguity; no kernel
    # release parsing/assumption or host-specific alternative is needed.
    for number in [*DENIED.values(), *range(512, 548)]:
        rows.extend([(0x15, 0, 1, number), (0x06, 0, 0, KILL)])

    def require_words(words):
        result = []
        for offset, expected in words:
            result.extend([(0x20, 0, 0, offset),
                           (0x15, 1, 0, expected), (0x06, 0, 0, KILL)])
        return result + [(0x06, 0, 0, ALLOW)]

    # seccomp_data.args start at byte16. Check both halves of flags and both
    # pointers. A NULL low word with a nonzero high word is NOT a NULL pointer.
    receive = require_words([(40, 0), (44, 0), (48, 0), (52, 0), (56, 0), (60, 0)])
    rows.extend([(0x15, 0, len(receive), 45), *receive])
    # Check full-width domain, even though the kernel consumes its low int.
    pair = require_words([(16, 1), (20, 0)])
    rows.extend([(0x15, 0, len(pair), 53), *pair, (0x06, 0, 0, ALLOW)])
    return rows


def guard(expected_arch=AUDIT_ARCH_X86_64):
    platform_check()
    if not sys.flags.isolated or not sys.flags.no_site:
        raise RuntimeError("Launch guard requires Python -I -S startup")
    single_thread_check()
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    # No outside socket endpoint may be inherited, including socket stdio.
    for text in os.listdir("/proc/self/fd"):
        fd = int(text)
        try:
            if fd > 2:
                os.close(fd)
            elif stat.S_ISSOCK(os.fstat(fd).st_mode):
                raise RuntimeError("Refusing inherited socket in standard descriptors")
        except OSError as error:
            if fd <= 2 or error.errno != errno.EBADF:
                raise
    single_thread_check()
    rows = filter_rows(expected_arch)
    filters = (Filter * len(rows))(*(Filter(*row) for row in rows))
    program = Program(len(rows), filters)
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(38, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "PR_SET_NO_NEW_PRIVS failed")
    if libc.syscall(317, 1, 0, ctypes.byref(program)) != 0:
        raise OSError(ctypes.get_errno(), "seccomp filter installation failed")
    if libc.prctl(21, 0, 0, 0, 0) != 2 or libc.prctl(39, 0, 0, 0, 0) != 1:
        raise RuntimeError("Seccomp/no_new_privs not active")


def raw(number, arguments):
    # Keep 64-bit high-half tests intact at the ctypes variadic boundary.
    libc = ctypes.CDLL(None, use_errno=True)
    libc.syscall.restype = ctypes.c_long
    return libc.syscall(ctypes.c_long(number),
                        *(ctypes.c_uint64(value & ((1 << 64) - 1)) for value in arguments))


def positive_pair():
    left, right = socket.socketpair()
    try:
        assert os.write(left.fileno(), b"owned local pair") == 16
        buffer = ctypes.create_string_buffer(32)
        assert raw(45, [right.fileno(), ctypes.addressof(buffer), 32, 0, 0, 0]) == 16
        assert buffer.raw[:16] == b"owned local pair"
        assert os.write(right.fileno(), b"x") == 1
        assert os.read(left.fileno(), 1) == b"x"
        assert raw(45, [-1, 0, 0, 0, 0, 0]) == -1
        assert ctypes.get_errno() == errno.EBADF
    finally:
        left.close()
        right.close()


def selftest():
    platform_check()
    path = os.path.abspath(__file__)
    python = [sys.executable, "-I", "-S"]
    # The CLI uses -I/-S for fresh, controlled interpreter startup. Importing
    # this module does not install a filter in the build/controller process.
    prefix = python + [path]

    def run(args, **kwargs):
        return subprocess.run(prefix + args, close_fds=True, timeout=30,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)

    def killed(label, args):
        result = run(args)
        if result.returncode != -signal.SIGSYS:
            raise RuntimeError(f"Launch guard {label}: expected SIGSYS, got {result.returncode}: {result.stderr!r}")

    for label, number in DENIED.items():
        killed(label, ["--probe", str(number), "-1", "0", "0", "0", "0", "0"])
    for number in [0x40000000 | 39, 0x40000000 | 45, *range(512, 548)]:
        killed(f"ABI {number}", ["--probe", str(number), "-1", "0", "0", "0", "0", "0"])
    for index in [3, 4, 5]:
        for nonzero in [1, 1 << 32]:
            arguments = [-1, 0, 0, 0, 0, 0]
            arguments[index] = nonzero
            killed(f"recvfrom arg{index}={nonzero}", ["--probe", "45", *map(str, arguments)])
    for domain in [0, 2, 10, 17, (1 << 32) | 1]:
        killed(f"socketpair domain {domain}", ["--probe", "53", str(domain), "1", "0", "0", "0", "0"])
    killed("audit architecture", ["--wrong-arch"])
    # Two explicit exec boundaries before syscall use; all denials include the
    # mandatory descriptor import/injection defenses, not merely socket().
    for label, number in DENIED.items():
        killed(f"post-exec {label}", ["--exec", sys.executable, "-I", "-S", path,
                                      "--inherited-exec-probe", str(number)])
    for mode in ["--positive", "--inherited-exec-positive"]:
        result = run(["--exec", sys.executable, "-I", "-S", path, mode])
        if result.returncode != 0:
            raise RuntimeError(f"Local recvfrom/inheritance failed: {result.stderr!r}")
    killed("new thread inherits filter", ["--thread-probe"])
    result = run(["--reject-existing-thread"])
    if result.returncode != 0 or b"existing thread rejected" not in result.stdout:
        raise RuntimeError(f"Existing sibling was not rejected: {result.stderr!r}")
    left, right = socket.socketpair()
    try:
        for fd in [0, 1, 2]:
            streams = {"stdin": subprocess.DEVNULL, "stdout": subprocess.PIPE, "stderr": subprocess.PIPE}
            streams[["stdin", "stdout", "stderr"][fd]] = left
            result = subprocess.run(prefix + ["--exec", sys.executable, "-I", "-S", "-c", "raise SystemExit(99)"],
                                    close_fds=True, timeout=30, **streams)
            diagnostic = result.stderr
            marker = b"Refusing inherited socket in standard descriptors"
            if fd == 2:
                # The child's diagnostic is on the deliberately inherited local
                # socket, not result.stderr. Do not accept an unrelated failure.
                diagnostic = bytearray()
                deadline = time.monotonic() + 2
                while marker not in diagnostic and len(diagnostic) < 65536:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        break
                    right.settimeout(remaining)
                    try:
                        chunk = right.recv(min(4096, 65536 - len(diagnostic)))
                    except TimeoutError:
                        break
                    if not chunk:
                        break
                    diagnostic.extend(chunk)
            if result.returncode != 1 or marker not in diagnostic:
                raise RuntimeError(f"Socket stdio {fd} was not rejected")
        result = run(["--exec", sys.executable, "-I", "-S", path, "--check-fds", str(left.fileno())],
                     pass_fds=(left.fileno(),))
        if result.returncode != 0:
            raise RuntimeError(f"Inherited extra fd survived: {result.stderr!r}")
    finally:
        left.close()
        right.close()
    print("Launch guard self-tests passed: 18 denied syscalls; exact local recvfrom; six nonzero argument halves; "
          "five forbidden pair domains; x32 bit and bare512-547; architecture; two exec boundaries; "
          "thread inheritance/single-thread install; socket stdio/extra-fd hygiene", flush=True)


def main():
    mode = sys.argv[1]
    if mode == "--selftest":
        selftest()
    elif mode == "--probe":
        guard()
        raw(int(sys.argv[2]), [int(value) for value in sys.argv[3:]])
        raise SystemExit("forbidden syscall unexpectedly returned")
    elif mode == "--wrong-arch":
        guard(0)
        raise SystemExit("wrong architecture unexpectedly allowed")
    elif mode == "--exec":
        guard()
        os.execv(sys.argv[2], sys.argv[2:])
    elif mode == "--positive":
        positive_pair()
    elif mode == "--inherited-exec-positive":
        os.execv(sys.executable, [sys.executable, "-I", "-S", os.path.abspath(__file__), "--positive"])
    elif mode == "--inherited-exec-probe":
        os.execv(sys.executable, [sys.executable, "-I", "-S", os.path.abspath(__file__),
                                 "--inherited-probe", sys.argv[2]])
    elif mode == "--inherited-probe":
        raw(int(sys.argv[2]), [-1, 0, 0, 0, 0, 0])
        raise SystemExit("inherited forbidden syscall unexpectedly returned")
    elif mode == "--check-fds":
        for fd in [0, 1, 2]:
            assert not stat.S_ISSOCK(os.fstat(fd).st_mode)
        try:
            os.fstat(int(sys.argv[2]))
        except OSError as error:
            assert error.errno == errno.EBADF
        else:
            raise RuntimeError("Extra inherited descriptor was not closed")
        assert len(os.listdir("/proc/self/fd")) == 4
    elif mode == "--thread-probe":
        guard()
        thread = threading.Thread(target=lambda: raw(41, [-1, 0, 0, 0, 0, 0]))
        thread.start()
        thread.join()
        raise SystemExit("new thread escaped filter")
    elif mode == "--reject-existing-thread":
        started, finish = threading.Event(), threading.Event()
        thread = threading.Thread(target=lambda: (started.set(), finish.wait()))
        thread.start()
        started.wait()
        try:
            try:
                guard()
            except RuntimeError as error:
                assert "fresh single-threaded" in str(error)
                print("existing thread rejected")
            else:
                raise RuntimeError("Guard accepted existing sibling")
        finally:
            finish.set()
            thread.join()
    else:
        raise SystemExit("Use --selftest or --exec /absolute/executable [args...]")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Linux x86_64 test-process-only network denial. Never changes system settings."""
import ctypes
import os
import platform
import resource
import signal
import socket
import stat
import subprocess
import sys

# x86_64 syscall numbers. Disallow network creation/use and io_uring, whose
# asynchronous socket operations would otherwise bypass individual syscall checks.
DENIED = {
    "socket": 41, "connect": 42, "accept": 43, "sendto": 44, "recvfrom": 45,
    "sendmsg": 46, "recvmsg": 47, "shutdown": 48, "bind": 49, "listen": 50,
    "accept4": 288, "recvmmsg": 299, "sendmmsg": 307,
    "io_uring_setup": 425, "io_uring_enter": 426, "io_uring_register": 427,
}
AUDIT_ARCH_X86_64 = 0xC000003E
KILL = 0x80000000  # SECCOMP_RET_KILL_PROCESS
ALLOW = 0x7FFF0000

class Filter(ctypes.Structure):
    _fields_ = [("code", ctypes.c_ushort), ("jt", ctypes.c_ubyte),
                ("jf", ctypes.c_ubyte), ("k", ctypes.c_uint)]

class Program(ctypes.Structure):
    _fields_ = [("len", ctypes.c_ushort), ("filter", ctypes.POINTER(Filter))]

def platform_check():
    if sys.platform != "linux" or platform.machine() != "x86_64" or ctypes.sizeof(ctypes.c_void_p) != 8:
        raise RuntimeError("Network guard supports only Linux x86_64 LP64; refusing unknown ABI")

def guard(expected_arch=AUDIT_ARCH_X86_64):
    platform_check()
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    # A fresh close_fds subprocess owns only stdio. Close any other descriptors
    # before installing the filter; never permit a preexisting socket as stdio.
    for text in os.listdir("/proc/self/fd"):
        fd = int(text)
        try:
            if fd > 2:
                os.close(fd)
            elif stat.S_ISSOCK(os.fstat(fd).st_mode):
                raise RuntimeError("Refusing inherited socket in standard descriptors")
        except OSError:
            if fd <= 2:
                raise
    # BPF: validate audit architecture first; reject x32 syscall bit before
    # checking x86_64 numbers. socketpair is limited to AF_UNIX for Tokio's
    # local signal wakeup pipe. No network socket can be created or inherited.
    rows = [(0x20,0,0,4), (0x15,1,0,expected_arch), (0x06,0,0,KILL),
            (0x20,0,0,0), (0x45,0,1,0x40000000), (0x06,0,0,KILL)]
    for number in DENIED.values():
        rows.extend([(0x15,0,1,number),(0x06,0,0,KILL)])
    rows.extend([(0x15,0,4,53),  # socketpair
                 (0x20,0,0,16), # args[0] low word: domain
                 (0x15,1,0,1),  # AF_UNIX only
                 (0x06,0,0,KILL),(0x06,0,0,ALLOW),
                 (0x06,0,0,ALLOW)])
    filters = (Filter * len(rows))(*(Filter(*r) for r in rows))
    program = Program(len(rows), filters)
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(38, 1, 0, 0, 0) != 0:  # PR_SET_NO_NEW_PRIVS
        raise OSError(ctypes.get_errno(), "PR_SET_NO_NEW_PRIVS failed")
    if libc.syscall(317, 1, 0, ctypes.byref(program)) != 0:  # seccomp SET_MODE_FILTER
        raise OSError(ctypes.get_errno(), "seccomp filter installation failed")
    if libc.prctl(21, 0, 0, 0, 0) != 2:  # PR_GET_SECCOMP
        raise RuntimeError("Seccomp filter mode not active")

def selftest():
    platform_check()
    path = os.path.abspath(__file__)
    # Execute raw syscalls with invalid descriptors/arguments: SIGSYS, rather
    # than EBADF/EINVAL, proves the filter intercepted the actual syscall.
    for label, number in [*DENIED.items(), ("x32",0x40000000 | 39),
                           ("non_unix_socketpair",53)]:
        result = subprocess.run([sys.executable,path,"--probe",str(number)],close_fds=True)
        if result.returncode != -signal.SIGSYS:
            raise RuntimeError(f"Guard self-test {label}: expected SIGSYS, got {result.returncode}")
    result = subprocess.run([sys.executable,path,"--wrong-arch"],close_fds=True)
    if result.returncode != -signal.SIGSYS:
        raise RuntimeError(f"Architecture guard self-test failed: {result.returncode}")
    # Verify the restrictions survive exec, as they must for the real Core.
    for label in ("socket", "connect"):
        result = subprocess.run([sys.executable,path,"--exec",sys.executable,"-c",
            f"import ctypes; ctypes.CDLL(None).syscall({DENIED[label]},-1,0,0,0,0,0)"],
            close_fds=True)
        if result.returncode != -signal.SIGSYS:
            raise RuntimeError(f"Post-exec {label} guard failed: {result.returncode}")
    subprocess.run([sys.executable,path,"--exec",sys.executable,"-c",
        "import os,socket; a,b=socket.socketpair(); "
        "assert os.write(a.fileno(),b'x') == 1; "
        "assert os.read(b.fileno(),1) == b'x'; a.close(); b.close()"],
        close_fds=True,check=True)
    # Local AF_UNIX pairs send no external traffic. Exercise both inherited
    # socket rejection as stdio and closing an explicitly inherited extra FD.
    left, right = socket.socketpair()
    try:
        result = subprocess.run([sys.executable,path,"--exec",sys.executable,"-c","pass"],
            stdin=left,stdout=subprocess.PIPE,stderr=subprocess.PIPE,close_fds=True)
        if result.returncode == 0 or b"Refusing inherited socket" not in result.stderr:
            raise RuntimeError("Inherited socket stdio was not rejected")
        subprocess.run([sys.executable,path,"--exec",sys.executable,"-c",
            "import os,stat; "
            "assert all(not stat.S_ISSOCK(os.fstat(fd).st_mode) for fd in (0,1,2)); "
            "assert len(os.listdir('/proc/self/fd')) == 4"],
            pass_fds=(left.fileno(),),close_fds=True,check=True)
    finally:
        left.close()
        right.close()
    print(f"Network guard self-tests passed: {len(DENIED)} blocked syscalls, x32, non-Unix socketpair, architecture, exec inheritance, local IPC, inherited FDs",flush=True)

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        selftest()
    elif len(sys.argv) > 1 and sys.argv[1] == "--wrong-arch":
        guard(0)
        raise SystemExit("wrong architecture unexpectedly allowed")
    elif len(sys.argv) > 2 and sys.argv[1] == "--probe":
        guard()
        ctypes.CDLL(None).syscall(int(sys.argv[2]), -1, 0, 0, 0, 0, 0)
        raise SystemExit("forbidden syscall unexpectedly allowed")
    elif len(sys.argv) > 2 and sys.argv[1] == "--exec":
        guard()
        os.execv(sys.argv[2],sys.argv[2:])
    else:
        raise SystemExit("Use --selftest or --exec /absolute/executable [args...]")

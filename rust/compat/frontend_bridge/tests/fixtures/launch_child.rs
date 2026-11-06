//! Benign native child, never a Java executable. Compiled offline by the opt-in
//! runner, copied into the owned fixture and executed only under its guard.
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::Duration,
};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    // Verify the parent's process-only filter survives both Core and child exec.
    assert!(fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .any(|line| line == "Seccomp:\t2"));
    for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        assert!(std::env::var_os(key).is_none());
    }
    if args[1..] == ["-XshowSettings:properties", "-version"] {
        eprintln!("java.specification.version = 17\nos.arch = amd64\nsun.arch.data.model = 64\njava.vendor = BenignNativeFixture");
        return;
    }
    assert!(std::env::var_os("JMCL_NATIVE_LAUNCH_CANARY").is_none());
    let main = args
        .iter()
        .position(|arg| arg == "NativeLaunchFixture")
        .unwrap();
    let tail = &args[main + 1..];
    assert_eq!(
        &tail[..6],
        [
            "synthetic-access-secret",
            "synthetic-session-secret",
            "Fixture 玩家",
            "synthetic-uuid",
            "literal argument",
            ";not shell"
        ]
    );
    let exit_code: i32 = tail[6].parse().unwrap();
    assert!(matches!(exit_code, 0 | 7));
    let native = args
        .iter()
        .find_map(|arg| arg.strip_prefix("-Djava.library.path="))
        .unwrap();
    assert!(Path::new(native).is_dir());
    let cwd = std::env::current_dir().unwrap();
    assert!(Path::new(native).starts_with(cwd.join(".jmcl/runtime/natives")));
    assert_eq!(
        fs::read("versions/synthetic/synthetic.jar").unwrap(),
        b"hello"
    );
    // An owned file proves native preparation existed while the process ran;
    // it must disappear with its entire per-launch directory after termination.
    fs::write(
        Path::new(native).join("child-canary"),
        b"owned native child",
    )
    .unwrap();
    let mut out = io::stdout().lock();
    out.write_all(b"native child ready\n").unwrap();
    out.write_all(&vec![b'x'; 16383]).unwrap();
    out.write_all(&[0xe4]).unwrap();
    out.flush().unwrap();
    std::thread::sleep(Duration::from_millis(10));
    out.write_all(&[0xb8, 0xad]).unwrap();
    out.write_all(&vec![b'y'; 20000]).unwrap();
    out.write_all(b"\r\n\ntoken=synthetic-access-").unwrap();
    out.flush().unwrap();
    std::thread::sleep(Duration::from_millis(10));
    out.write_all(b"secret; session=synthetic-session-secret\n")
        .unwrap();
    out.flush().unwrap();
    let mut err = io::stderr().lock();
    err.write_all(b"stderr=synthetic-session-").unwrap();
    err.flush().unwrap();
    std::thread::sleep(Duration::from_millis(10));
    err.write_all(b"secret\r\nbinary\0\xff\r").unwrap();
    err.flush().unwrap();
    std::process::exit(exit_code);
}

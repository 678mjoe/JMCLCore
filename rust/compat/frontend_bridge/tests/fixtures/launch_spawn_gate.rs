//! Mandatory benign spawn gate for the separately reviewed launch-only guard.
//! No Java, shell, download, external account or network operation.
use std::{fs, io::ErrorKind, os::unix::process::CommandExt, process::Command};

fn main() {
    assert!(fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .any(|line| line == "Seccomp:\t2"));
    let mode = std::env::args().nth(1).unwrap();
    if mode == "child" {
        println!("benign child started");
        return;
    }
    let executable = std::env::current_exe().unwrap();
    let mut command = if mode == "missing" {
        let missing = executable.with_file_name("owned-missing-native-child");
        assert!(!missing.exists());
        Command::new(missing)
    } else {
        assert_eq!(mode, "success");
        let mut command = Command::new(executable);
        command.arg("child");
        command
    };
    // Exercise the std fork/pre_exec error channel; this callback is inert.
    unsafe {
        command.pre_exec(|| Ok(()));
    }
    if mode == "missing" {
        assert_eq!(command.spawn().unwrap_err().kind(), ErrorKind::NotFound);
        println!("missing spawn returned NotFound");
    } else {
        assert_eq!(command.spawn().unwrap().wait().unwrap().code(), Some(0));
        println!("successful spawn returned exit 0");
    }
}

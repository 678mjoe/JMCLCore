//! Standalone diagnosis only, not a Cargo test or Java fixture. The existing
//! network_guard.py must wrap this executable; never run it unguarded.
use std::{fs, os::unix::process::CommandExt, process::Command};

fn main() {
    assert!(fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .any(|line| line == "Seccomp:\t2"));
    let mode = std::env::args().nth(1).unwrap();
    if mode == "child" {
        eprintln!("benign child started");
        return;
    }
    let mut command = if mode == "missing" {
        Command::new("/nonexistent-jmcl-owned-diagnostic-child")
    } else {
        assert_eq!(mode, "success");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.arg("child");
        command
    };
    // Exactly the std spawn branch that Core's direct_exec probe requires.
    // This closure has no side effect and cannot create network traffic.
    unsafe {
        command.pre_exec(|| Ok(()));
    }
    eprintln!("before {mode} child spawn");
    eprintln!(
        "spawn: {:?}",
        command.spawn().and_then(|mut child| child.wait())
    );
}

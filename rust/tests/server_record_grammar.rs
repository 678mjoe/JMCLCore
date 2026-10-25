//! Portable typed-parser reference check. No server/process-control or Java path.
//! Run with Zig 0.16.0 on PATH (or JMCL_ZIG_COMPILER) and --ignored.
use std::{path::Path, process::Command};
#[test]
#[ignore = "requires Zig 0.16.0 on PATH or JMCL_ZIG_COMPILER"]
fn pinned_zig_typed_server_record_grammar() {
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&zig).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let cache = tempfile::tempdir().unwrap();
    let output = Command::new(&zig)
        .arg("test")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server_record_numbers.zig"))
        .args(["-O", "ReleaseSafe", "--cache-dir"])
        .arg(cache.path().join("local"))
        .arg("--global-cache-dir")
        .arg(cache.path().join("global"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
}

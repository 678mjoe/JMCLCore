//! Native Zig/Rust CLI differential corpus; intended on Linux/macOS/Windows.
//! Unlike RPC, selected server-world CLI operations do not require Linux.
use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn setup(path: &Path) {
    fs::create_dir_all(path.join("demo/custom")).unwrap();
    fs::create_dir_all(path.join("demo/custom_nether")).unwrap();
    fs::create_dir_all(path.join("demo/backups")).unwrap();
    fs::write(
        path.join("demo/server.json"),
        br#"{"id":"demo","name":"Demo","version_id":"fixture"}"#,
    )
    .unwrap();
    fs::write(path.join("demo/server.properties"), b"level-name=custom\n").unwrap();
    fs::write(path.join("demo/custom/data"), b"mutable world").unwrap();
    fs::write(path.join("demo/backups/fixture.zip"), b"synthetic backup").unwrap();
}
fn output(exe: &Path, args: &[&str], root: &Path) -> (i32, String, String) {
    let out = Command::new(exe)
        .args(["server", "worlds"])
        .args(args)
        .arg("--servers-directory")
        .arg(root)
        .output()
        .unwrap();
    let mut text = String::from_utf8(out.stdout).unwrap();
    if let Ok(mut value) = serde_json::from_str::<Value>(&text) {
        if let Some(backups) = value["backups"].as_array_mut() {
            for b in backups {
                b.as_object_mut().unwrap().remove("created_ms");
            }
        }
        text = value.to_string();
    }
    (
        out.status.code().unwrap(),
        text,
        String::from_utf8(out.stderr).unwrap(),
    )
}
#[test]
#[ignore = "requires the native pinned JMCL_ZIG_ORACLE executable"]
fn server_world_cli_native_platform_asymmetry() {
    let oracle = std::env::var_os("JMCL_ZIG_ORACLE").expect("explicit native oracle required");
    let oracle = Path::new(&oracle);
    let rust = Path::new(env!("CARGO_BIN_EXE_jmcl-core-rust"));
    let cases: &[&[&str]] = &[
        &[],
        &["unknown"],
        &["list"],
        &["show"],
        &["backups"],
        &["backups-delete"],
        &["list", "absent"],
        &["show", "absent", "custom"],
        &["list", "bad/id"],
        &["list", "demo", "--json"],
        &["list", "demo"],
        &["show", "demo", "custom", "--json"],
        &["show", "demo", "custom"],
        &["show", "demo", "absent"],
        &["show", "demo", "../bad"],
        &["show", "demo"],
        &["backups", "absent"],
        &["backups", "demo", "--json"],
        &["backups-delete", "absent", "fixture.zip"],
        &["backups-delete", "demo", "fixture.zip"],
        &["backups-delete", "demo", "missing.zip"],
        &["backups-delete", "demo", "../bad"],
        &["rename", "absent", "custom", "new"],
        &["rename", "demo", "custom", "new"],
        &["delete", "absent", "custom"],
        &["delete", "demo", "custom"],
        &["backup", "absent"],
        &["backup", "demo", "bad/name"],
        &["restore", "absent", "bad/name"],
        &["restore", "demo", "missing.zip"],
        &["list", "demo", "extra"],
        &["show", "demo", "custom", "extra"],
        &["list", "demo", "--label", "unused"],
    ];
    for args in cases {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        setup(left.path());
        setup(right.path());
        let expected = output(oracle, args, left.path());
        let actual = output(rust, args, right.path());
        assert_eq!(actual, expected, "native server worlds args={args:?}");
        // Read-only listing and backup administration must never create a
        // lifecycle lock/PID or turn mutable world bytes into CAS links.
        if matches!(
            args.first(),
            Some(&"list" | &"show" | &"backups" | &"backups-delete")
        ) {
            assert!(!right.path().join("demo/.jmcl").exists());
        }
    }
}

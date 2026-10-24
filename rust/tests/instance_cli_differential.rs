use jmcl_core_rust::instances;
use std::process::Command;
fn rust(args: &[String]) -> (u8, Vec<u8>, Vec<u8>) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = instances::cli(args, &mut out, &mut err)
        .expect("handled command")
        .unwrap();
    (status, out, err)
}
fn zig(args: &[String]) -> (u8, Vec<u8>, Vec<u8>) {
    let o = Command::new(std::env::var("JMCL_ZIG_ORACLE").expect("pinned Zig oracle required"))
        .arg("instance")
        .args(args)
        .output()
        .unwrap();
    (o.status.code().unwrap() as u8, o.stdout, o.stderr)
}
fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}
#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn cli_raw_lifecycle() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().to_str().unwrap();
    let create = args(&[
        "create",
        "a",
        "--version",
        "1.21",
        "--name",
        "Test 世界",
        "--source",
        "bmclapi",
        "--fabric-loader",
        "0.16.10",
        "--instances-directory",
        dir,
        "--no-validate",
    ]);
    let delete = args(&["delete", "a", "--force", "--instances-directory", dir]);
    let r = rust(&create);
    assert_eq!(r.0, 0);
    assert_eq!(rust(&delete).0, 0);
    let z = zig(&create);
    assert_eq!(r, z);
    for op in [
        args(&["list", "--instances-directory", dir]),
        args(&["list", "--instances-directory", dir, "--json"]),
        args(&["show", "a", "--instances-directory", dir]),
        args(&["show", "a", "--instances-directory", dir, "--json"]),
    ] {
        assert_eq!(rust(&op), zig(&op), "{op:?}");
    }
    let r = rust(&delete);
    assert_eq!(rust(&create).0, 0);
    let z = zig(&delete);
    assert_eq!(r, z);
}
#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn cli_invalid_args_and_missing_files() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().to_str().unwrap();
    for a in [
        args(&["list", "--bad"]),
        args(&["create"]),
        args(&["create", "bad id", "--version", "1.21"]),
        args(&[
            "create",
            "a",
            "--version",
            "1.21",
            "--fabric-loader",
            "a",
            "--forge-version",
            "b",
            "--no-validate",
        ]),
        args(&["delete", "a"]),
        args(&["delete", "a", "--json"]),
        args(&["show", "a", "--instances-directory", dir]),
        args(&["delete", "a", "--force", "--instances-directory", dir]),
    ] {
        assert_eq!(rust(&a), zig(&a), "{a:?}");
    }
}

#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn cli_manifest_error_names() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir(t.path().join("a")).unwrap();
    let a = args(&[
        "show",
        "a",
        "--instances-directory",
        t.path().to_str().unwrap(),
    ]);
    for data in [
        "",
        "{",
        "[]",
        "null",
        "{}",
        "x",
        "{\"id\":1}",
        "{\"id\":null}",
        r#"{"id":"a","name":"A","version_id":"1","source":"wat"}"#,
        r#"{"id":"a","id":"a","name":"A","version_id":"1"}"#,
        r#"{"id":"a","name":"A","version_id":"1","schema_version":"wat"}"#,
        r#"{"id":"a","name":"A","version_id":"1","schema_version":1.5}"#,
        r#"{"id":"a","name":"A","version_id":"1","schema_version":-1}"#,
    ] {
        std::fs::write(t.path().join("a/instance.json"), data).unwrap();
        assert_eq!(rust(&a), zig(&a), "{data}");
    }
}

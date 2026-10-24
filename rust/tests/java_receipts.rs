use jmcl_core_rust::java::{receipt, store};
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; CI must explicitly execute this compatibility gate"]
fn rust_receipt_is_directly_readable_by_zig() {
    let oracle = std::env::var_os("JMCL_ZIG_ORACLE").expect("pinned Zig oracle required");
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("store");
    let runtime = root.join("runtimes/mojang-fixture-linux");
    let bytes=br#"{"provider":"mojang","id":"fixture","platform":"linux","major_version":17,"java_home":"","executable":"bin/java","source_url":"https://example.invalid/runtime","installed_at_ms":123}"#;
    let r = receipt::parse(bytes).unwrap();
    store::write_receipt(&runtime, &r).unwrap();
    let written = std::fs::read(runtime.join(receipt::NAME)).unwrap();
    assert_eq!(written, serde_json::to_vec(&r).unwrap());
    let out = std::process::Command::new(oracle)
        .args([
            "java",
            "runtime",
            "list",
            "--store",
            root.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["result"]["runtimes"][0], r.cli("mojang-fixture-linux"));
    let list = store::list(root.to_str().unwrap()).unwrap();
    assert_eq!(list[0].receipt, r);
}

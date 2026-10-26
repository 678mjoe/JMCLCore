//! No-state LIST validation view only; no lifecycle or installation.
#![cfg(target_os = "linux")]
use jmcl_core_rust::server_content;
use serde_json::{json, value::RawValue};
#[test]
fn list_accepts_opaque_manifest_display_name_without_rewrite() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("sample");
    std::fs::create_dir(&home).unwrap();
    let bytes = br#"{"id":"sample","name":[255,0,10],"version_id":"fake"}"#;
    std::fs::write(home.join("server.json"), bytes).unwrap();
    let p = RawValue::from_string(json!({"directory":t.path(),"id":"sample"}).to_string()).unwrap();
    let mut out = vec![];
    server_content::handle("server.mods.list", Some(&p), "t", &mut out)
        .unwrap()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["event"], "result");
    assert_eq!(std::fs::read(home.join("server.json")).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 1);
}

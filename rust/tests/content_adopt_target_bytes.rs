//! Owned blocked roots prove decoding/error order without importing any content.
use jmcl_core_rust::content;
use serde_json::{json, value::RawValue};
#[test]
fn adoption_target_arrays_reach_root_before_store_without_creation() {
    let t = tempfile::tempdir().unwrap();
    let blocked = t.path().join("blocked");
    std::fs::write(&blocked, b"owned sentinel").unwrap();
    let store = t.path().join("unused-store");
    let p = RawValue::from_string(json!({"directory":blocked.to_str().unwrap().as_bytes(),"store_directory":store.to_str().unwrap().as_bytes()}).to_string()).unwrap();
    let error = content::handle("mods.adopt", Some(&p))
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "INSTALL_ROOT_ERROR");
    assert_eq!(std::fs::read(blocked).unwrap(), b"owned sentinel");
    assert!(!store.exists());
}

//! List-only owned root fixtures; no artifact mutation or acquisition.
use jmcl_core_rust::content;
use serde_json::{json, value::RawValue};
#[test]
fn ascii_directory_array_lists_existing_root() {
    let t = tempfile::tempdir().unwrap();
    let raw = RawValue::from_string(
        json!({"directory":t.path().to_str().unwrap().as_bytes()}).to_string(),
    )
    .unwrap();
    let result = content::handle("mods.list", Some(&raw)).unwrap().unwrap();
    assert_eq!(
        result,
        json!({"stage":"mods.list","entries":[],"unmanaged":[]})
    );
    assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
}
#[test]
fn mutation_directory_arrays_reach_selector_lookup_without_creating_files() {
    let t = tempfile::tempdir().unwrap();
    let raw = RawValue::from_string(
        json!({"directory":t.path().to_str().unwrap().as_bytes(),"file":"mods/a.jar"}).to_string(),
    )
    .unwrap();
    for method in ["mods.enable", "mods.disable", "mods.remove"] {
        let e = content::handle(method, Some(&raw)).unwrap().unwrap_err();
        assert_eq!(e.code, "CONTENT_NOT_FOUND");
    }
    assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
}

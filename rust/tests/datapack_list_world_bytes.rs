//! Read-only existing-world parameter checks; no world creation by the core.
use jmcl_core_rust::content;
use serde_json::{json, value::RawValue};
#[test]
fn list_world_ascii_array_preserves_wire_name_and_read_order() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("saves/World/datapacks")).unwrap();
    let p = RawValue::from_string(
        json!({"directory":t.path().to_str().unwrap(),"world":b"World"}).to_string(),
    )
    .unwrap();
    assert_eq!(
        content::handle("datapacks.list", Some(&p))
            .unwrap()
            .unwrap(),
        json!({"stage":"datapacks.list","world":"World","entries":[],"unmanaged":[]})
    );
    let absent = tempfile::tempdir().unwrap();
    let p = RawValue::from_string(
        json!({"directory":absent.path().to_str().unwrap(),"world":b"../bad"}).to_string(),
    )
    .unwrap();
    assert_eq!(
        content::handle("datapacks.list", Some(&p))
            .unwrap()
            .unwrap_err()
            .code,
        "WORLD_NOT_FOUND"
    );
    assert_eq!(std::fs::read_dir(absent.path()).unwrap().count(), 0);
}
#[test]
fn mutation_world_arrays_reach_lookup_without_creating_content() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("saves/World")).unwrap();
    for method in ["datapacks.enable", "datapacks.disable", "datapacks.remove"] {
        let p = RawValue::from_string(json!({"directory":t.path().to_str().unwrap(),"world":b"World","file":"datapacks/a.zip"}).to_string()).unwrap();
        assert_eq!(
            content::handle(method, Some(&p)).unwrap().unwrap_err().code,
            "CONTENT_NOT_FOUND"
        );
    }
    assert_eq!(
        std::fs::read_dir(t.path().join("saves/World"))
            .unwrap()
            .count(),
        0
    );
}

//! No-state metadata validation only; malformed guard proves ordering without locking.
#[cfg(target_os = "linux")]
#[test]
fn local_display_bytes_validate_before_guard_without_state() {
    use jmcl_core_rust::server_content;
    use serde_json::{json, value::RawValue};
    let t = tempfile::tempdir().unwrap();
    let server = t.path().join("demo");
    std::fs::create_dir_all(server.join(".jmcl/lifecycle.lock")).unwrap();
    for name in [json!([255]), json!([])] {
        std::fs::write(
            server.join("server.json"),
            json!({"id":"bad id","name":name,"version_id":"1.20.1"}).to_string(),
        )
        .unwrap();
        let p = RawValue::from_string(
            json!({"directory":t.path(),"id":"demo","file":"mods/a.jar"}).to_string(),
        )
        .unwrap();
        for family in ["mods", "datapacks"] {
            for op in ["enable", "disable", "remove"] {
                let mut out = vec![];
                server_content::handle(&format!("server.{family}.{op}"), Some(&p), "t", &mut out)
                    .unwrap()
                    .unwrap();
                let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
                assert_eq!(v["error"]["code"], "SERVER_NOT_FOUND");
                assert!(!server.join(".jmcl/server.pid").exists());
            }
        }
    }
}

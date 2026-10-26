#[cfg(target_os = "linux")]
#[test]
fn adoption_display_bytes_preserve_stored_id_precedence() {
    use jmcl_core_rust::server_content;
    use serde_json::{json, value::RawValue};
    let t = tempfile::tempdir().unwrap();
    let servers = t.path().join("servers");
    let dir = servers.join("demo");
    std::fs::create_dir_all(dir.join(".jmcl/lifecycle.lock")).unwrap();
    let cas = t.path().join("cas");
    std::fs::create_dir_all(cas.join("objects/sha1")).unwrap();
    std::fs::create_dir_all(cas.join("locks/sha1")).unwrap();
    for name in [json!([255]), json!([])] {
        std::fs::write(
            dir.join("server.json"),
            json!({"id":"bad id","name":name,"version_id":"1.20.1"}).to_string(),
        )
        .unwrap();
        let p = RawValue::from_string(
            json!({"directory":servers,"id":"demo","store_directory":cas}).to_string(),
        )
        .unwrap();
        for family in ["mods", "datapacks"] {
            let mut out = vec![];
            server_content::handle(&format!("server.{family}.adopt"), Some(&p), "t", &mut out)
                .unwrap()
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(v["error"]["code"], "SERVER_NOT_FOUND");
            assert!(!dir.join(".jmcl/server.pid").exists());
            assert_eq!(
                std::fs::read_dir(cas.join("objects/sha1")).unwrap().count(),
                0
            );
        }
    }
}

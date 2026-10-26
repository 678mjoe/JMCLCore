#[cfg(target_os = "linux")]
#[test]
fn server_adoption_array_range_precedes_explicit_owned_store() {
    use jmcl_core_rust::server_content;
    use serde_json::{json, value::RawValue};
    let t = tempfile::tempdir().unwrap();
    let cas = t.path().join("cas");
    for family in ["mods", "datapacks"] {
        let p = RawValue::from_string(
            json!({"directory":[],"id":b"demo","store_directory":cas.to_str().unwrap()})
                .to_string(),
        )
        .unwrap();
        let mut out = vec![];
        server_content::handle(&format!("server.{family}.adopt"), Some(&p), "t", &mut out)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["error"]["message"],
            format!("server {family} adoption requires directory and id")
        );
        assert!(!cas.exists());
    }
}

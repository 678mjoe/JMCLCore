//! Owned blocked roots; no server state, process identities or guard acquisition.
use jmcl_core_rust::server_content;
use serde_json::{json, value::RawValue};
#[cfg(target_os = "linux")]
#[test]
fn local_target_arrays_reach_root_without_creating_server_state() {
    let t = tempfile::tempdir().unwrap();
    let blocked = t.path().join("blocked");
    std::fs::write(&blocked, b"owned").unwrap();
    let p = RawValue::from_string(
        json!({"directory":blocked.to_str().unwrap().as_bytes(),"id":b"demo","file":"mods/a.jar"})
            .to_string(),
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
        }
    }
    assert_eq!(std::fs::read(blocked).unwrap(), b"owned");
    assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
}
#[cfg(not(target_os = "linux"))]
#[test]
fn platform_gate_still_precedes_local_target_decoding() {
    let p = RawValue::from_string(json!({"directory":[256],"id":[]}).to_string()).unwrap();
    for family in ["mods", "datapacks"] {
        for op in ["enable", "disable", "remove"] {
            let mut out = vec![];
            server_content::handle(&format!("server.{family}.{op}"), Some(&p), "t", &mut out)
                .unwrap()
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(v["error"]["code"], "UNSUPPORTED_PLATFORM");
        }
    }
}

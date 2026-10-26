//! No-state LIST fixtures only: no lifecycle, signals, acquisition or mutations.
use jmcl_core_rust::server_content;
use serde_json::{json, value::RawValue};
#[cfg(target_os = "linux")]
#[test]
fn server_list_accepts_ascii_target_arrays_without_state() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("sample");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(
        home.join("server.json"),
        br#"{"id":"sample","name":"Name","version_id":"fake"}"#,
    )
    .unwrap();
    let p = RawValue::from_string(
        json!({"directory":t.path().to_str().unwrap().as_bytes(),"id":b"sample"}).to_string(),
    )
    .unwrap();
    let mut out = vec![];
    server_content::handle("server.mods.list", Some(&p), "t", &mut out)
        .unwrap()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["event"], "result");
    assert_eq!(
        v["result"],
        json!({"stage":"server.mods.list","id":"sample","entries":[],"unmanaged":[]})
    );
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 1);
}
#[cfg(not(target_os = "linux"))]
#[test]
fn list_platform_gate_still_precedes_parameter_decoding() {
    let p = RawValue::from_string(json!({}).to_string()).unwrap();
    let mut out = vec![];
    server_content::handle("server.mods.list", Some(&p), "t", &mut out)
        .unwrap()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["error"]["code"], "UNSUPPORTED_PLATFORM");
}

#[cfg(target_os = "linux")]
#[test]
fn list_null_parameters_keep_missing_parameter_diagnostic() {
    let p = RawValue::from_string("null".into()).unwrap();
    let mut out = vec![];
    server_content::handle("server.mods.list", Some(&p), "t", &mut out)
        .unwrap()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        v["error"],
        json!({"code":"INVALID_PARAMS","message":"missing parameters"})
    );
}

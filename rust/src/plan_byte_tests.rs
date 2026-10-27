use super::*;
use crate::metadata;
use serde_json::json;
fn target() -> Target {
    Target {
        os: "linux".into(),
        arch: "x86_64".into(),
        os_version: None,
    }
}
fn raw(id: Value) -> Value {
    json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":"https://piston-data.mojang.com/client","sha1":"a".repeat(40),"size":10}}})
}
fn normalize(v: &Value) -> Value {
    metadata::metadata(&serde_json::to_vec(v).unwrap()).unwrap()
}
fn rich(id: Value) -> Value {
    let mut v = raw(id);
    v["assetIndex"] = json!({"id":"index","url":"https://example.invalid/index","sha1":"b".repeat(40),"size":2,"totalSize":3});
    v["logging"] = json!({"client":{"argument":"-Dlog=${path}","type":"log4j2-xml","file":{"id":"log.xml","url":"https://example.invalid/log","sha1":"c".repeat(40),"size":4}}});
    v["libraries"] = json!([{"name":"example:lib:1","downloads":{"artifact":{"path":"example/lib.jar","url":"https://example.invalid/lib","sha1":"d".repeat(40),"size":5}}}]);
    v
}
fn assets() -> Value {
    metadata::asset_index(&serde_json::to_vec(&json!({"objects":{"sound/a":{"hash":"e".repeat(40),"size":3}},"virtual":true,"map_to_resources":true})).unwrap()).unwrap()
}
fn string_cases() -> Value {
    let mut results = Vec::new();
    for (id, source) in [("1.21", Source::Official), ("版本", Source::Bmclapi)] {
        results.push(json!({"name":id,"result":serde_json::to_string(&build(&normalize(&raw(id.into())),None,source,target()).unwrap()).unwrap()}));
    }
    let mut v = rich("demo".into());
    results.push(json!({"name":"rich","result":serde_json::to_string(&build(&normalize(&v),Some(&assets()),Source::Official,target()).unwrap()).unwrap()}));
    v["patched_client"] = "patched/client.jar".into();
    results.push(json!({"name":"patched","result":serde_json::to_string(&build(&normalize(&v),Some(&assets()),Source::Bmclapi,target()).unwrap()).unwrap()}));
    for (name, id, url, hash) in [
        ("path-first", "../bad", "http://x", "bad"),
        ("url-first", "demo", "http://x", "bad"),
        ("hash", "demo", "https://x", "bad"),
    ] {
        let mut v = raw(id.into());
        v["downloads"]["client"]["url"] = url.into();
        v["downloads"]["client"]["sha1"] = hash.into();
        results.push(json!({"name":name,"error":build(&normalize(&v),None,Source::Official,target()).unwrap_err().name()}));
    }
    results.push(json!({"name":"unexpected-assets","error":build(&normalize(&raw("demo".into())),Some(&assets()),Source::Official,target()).unwrap_err().name()}));
    Value::Array(results)
}
#[test]
fn ordinary_string_plan_baseline_is_complete() {
    assert_eq!(string_cases(), serde_json::from_str::<Value>(include_str!("../compat/plan-string-baseline.json")).unwrap());
}

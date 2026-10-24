#![cfg(target_os = "linux")]
use jmcl_core_rust::servers::{manifest::Manifest, properties};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn zig(method: &str, p: Value) -> Value {
    let mut c = Command::new(std::env::var("JMCL_ZIG_ORACLE").expect("pinned oracle required"))
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        c.stdin.take().unwrap(),
        "{}",
        json!({"protocol":1,"id":"t","method":method,"params":p})
    )
    .unwrap();
    let o = c.wait_with_output().unwrap();
    assert!(o.status.success());
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
#[ignore = "requires pinned Zig oracle executable; synthetic documents only"]
fn rust_server_manifest_and_properties_are_read_by_zig() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("servers");
    std::fs::create_dir_all(dir.join("a")).unwrap();
    let m=Manifest::parse(br#"{"id":"a","name":"A","version_id":"1.21","fabric_loader":"0.16.10","source":"bmclapi"}"#,"a").unwrap();
    std::fs::write(dir.join("a/server.json"), m.encode().unwrap()).unwrap();
    let p = json!({"directory":dir,"id":"a"});
    let got = zig("server.get", p.clone());
    assert_eq!(got["event"], "result");
    assert_eq!(got["result"]["id"], m.id);
    assert_eq!(got["result"]["installed"], false);
    for bytes in [properties::DEFAULT_CONTENT.as_bytes(),b"# comment\nescaped\\ key=value\\ with\\ spaces\ncontinued=first\\\n second\nunicode=\\u4f60\\u597d\\uD83D\\uDE00\nduplicate=first\nduplicate:second\nrcon.password=synthetic-placeholder\n".as_slice()]{let parsed=properties::parse(bytes).unwrap();std::fs::write(dir.join("a/server.properties"),parsed.encode()).unwrap();let got=zig("server.properties.get",p.clone());assert_eq!(got["event"],"result");let mut expected=serde_json::Map::new();for e in parsed.entries{expected.insert(e.key.clone(),json!(if e.key=="rcon.password"{"<redacted>"}else{&e.value}));}assert_eq!(got["result"]["properties"].to_string(),Value::Object(expected).to_string());}
}

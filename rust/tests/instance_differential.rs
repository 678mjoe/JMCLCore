//! Explicit oracle gate: run with JMCL_ZIG_ORACLE pointing to the pinned executable.
use jmcl_core_rust::instances;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn rust(method: &str, p: Option<Value>) -> Value {
    let raw = p.map(|p| serde_json::value::RawValue::from_string(p.to_string()).unwrap());
    match instances::handle(method, raw.as_deref()).unwrap() {
        Ok(result) => json!({"protocol":1,"id":"t","event":"result","result":result}),
        Err(e) => {
            json!({"protocol":1,"id":"t","event":"error","error":{"code":e.code,"message":e.message}})
        }
    }
}
fn zig(method: &str, p: Option<Value>) -> Value {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").expect("set JMCL_ZIG_ORACLE to pinned Zig Core");
    let mut c = Command::new(oracle)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut req = json!({"protocol":1,"id":"t","method":method});
    if let Some(p) = p {
        req["params"] = p;
    }
    writeln!(c.stdin.take().unwrap(), "{req}").unwrap();
    let o = c.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn shared_directory_bidirectional() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("instances").to_string_lossy().into_owned();
    let create = json!({"directory":dir,"id":"rust","version_id":"1.21","name":"Rust-created","fabric_loader":"0.16.10","source":"bmclapi","validate":false});
    let a = rust("instance.create", Some(create.clone()));
    assert_eq!(a["event"], "result");
    let target = json!({"directory":dir,"id":"rust"});
    assert_eq!(
        rust("instance.get", Some(target.clone())),
        zig("instance.get", Some(target.clone()))
    );
    let mut z = create.clone();
    z["id"] = json!("zig");
    z["name"] = json!("Zig-created");
    assert_eq!(zig("instance.create", Some(z))["event"], "result");
    let list = json!({"directory":dir});
    assert_eq!(
        rust("instance.list", Some(list.clone())),
        zig("instance.list", Some(list.clone()))
    );
    std::fs::create_dir_all(t.path().join("instances/rust/.minecraft/.jmcl")).unwrap();
    std::fs::write(t.path().join("instances/rust/.minecraft/.jmcl/install.json"),br#"{"version_id":"1.21","fabric_loader":"0.16.10","source":"official","os":"linux","arch":"x86_64"}"#).unwrap();
    assert_eq!(
        rust("instance.get", Some(target.clone())),
        zig("instance.get", Some(target.clone()))
    );
    // A Rust-created manifest is accepted and deleted by Zig; the converse also works.
    assert_eq!(zig("instance.delete", Some(target))["event"], "result");
    assert_eq!(
        rust("instance.delete", Some(json!({"directory":dir,"id":"zig"})))["event"],
        "result"
    );
    assert_eq!(
        rust("instance.list", Some(list.clone())),
        zig("instance.list", Some(list))
    );
}
#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn invalid_parameter_and_disk_corpus() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().to_string_lossy().into_owned();
    for method in [
        "instance.list",
        "instance.get",
        "instance.delete",
        "instance.create",
    ] {
        for p in [
            None,
            Some(json!({})),
            Some(json!([])),
            Some(json!({"directory":""})),
            Some(json!({"directory":dir,"id":"../bad","version_id":"1.21","validate":false})),
            Some(
                json!({"directory":dir,"id":"missing","version_id":"1.21","fabric_loader":"a","forge_version":"b","validate":false}),
            ),
        ] {
            assert_eq!(
                rust(method, p.clone()),
                zig(method, p.clone()),
                "{method} {p:?}"
            );
        }
    }
    for (id, contents) in [
        (
            "bad",
            br#"{"id":"bad","name":"A","version_id":"1.21","schema_version":99}"#.as_slice(),
        ),
        (
            "mismatch",
            br#"{"id":"other","name":"A","version_id":"1.21"}"#.as_slice(),
        ),
    ] {
        std::fs::create_dir(t.path().join(id)).unwrap();
        std::fs::write(t.path().join(id).join("instance.json"), contents).unwrap();
        for method in ["instance.get", "instance.delete"] {
            let p = Some(json!({"directory":dir,"id":id}));
            assert_eq!(rust(method, p.clone()), zig(method, p));
        }
    }
}

#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn schema_numeric_compatibility() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().to_string_lossy().into_owned();
    std::fs::create_dir(t.path().join("a")).unwrap();
    std::fs::create_dir(t.path().join("a/.minecraft")).unwrap();
    for version in [
        json!("1"),
        json!(1.0),
        json!(null),
        json!("1e0"),
        json!("01"),
        json!(2),
    ] {
        std::fs::write(
            t.path().join("a/instance.json"),
            json!({"schema_version":version,"id":"a","name":"A","version_id":"1.21"}).to_string(),
        )
        .unwrap();
        let p = Some(json!({"directory":dir,"id":"a"}));
        assert_eq!(
            rust("instance.get", p.clone()),
            zig("instance.get", p),
            "{version}"
        );
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn configured_root_symlink_ancestor_matches_oracle() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir(t.path().join("real")).unwrap();
    symlink(t.path().join("real"), t.path().join("alias")).unwrap();
    let dir = t
        .path()
        .join("alias/instances")
        .to_string_lossy()
        .into_owned();
    let p = Some(json!({"directory":dir,"id":"a","version_id":"1.21","validate":false}));
    let created = rust("instance.create", p);
    assert_eq!(created["event"], "result");
    let p = Some(json!({"directory":dir,"id":"a"}));
    assert_eq!(
        rust("instance.get", p.clone()).to_string(),
        zig("instance.get", p).to_string()
    );
    let p = Some(json!({"directory":dir}));
    assert_eq!(
        rust("instance.list", p.clone()).to_string(),
        zig("instance.list", p).to_string()
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires pinned Zig oracle executable"]
fn manifest_read_alias_does_not_redirect_deletion() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("instances");
    std::fs::create_dir_all(dir.join("a/.minecraft")).unwrap();
    let outside = t.path().join("outside.json");
    let bytes = br#"{"id":"a","name":"Alias","version_id":"1.21"}"#;
    std::fs::write(&outside, bytes).unwrap();
    symlink(&outside, dir.join("a/instance.json")).unwrap();
    let p = Some(json!({"directory":dir,"id":"a"}));
    assert_eq!(
        rust("instance.get", p.clone()).to_string(),
        zig("instance.get", p.clone()).to_string()
    );
    std::fs::write(
        &outside,
        br#"{"id":"other","name":"Alias","version_id":"1.21"}"#,
    )
    .unwrap();
    assert_eq!(
        rust("instance.get", p.clone()),
        zig("instance.get", p.clone())
    );
    std::fs::write(&outside, bytes).unwrap();
    assert_eq!(rust("instance.delete", p)["event"], "result");
    assert_eq!(std::fs::read(&outside).unwrap(), bytes);
    assert!(!dir.join("a").exists());
}

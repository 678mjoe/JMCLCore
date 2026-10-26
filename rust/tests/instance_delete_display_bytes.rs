//! Owned disposable instance.delete fixtures only; no lifecycle or installation.
use jmcl_core_rust::instances;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn manifest(name: Value) -> Value {
    json!({"schema_version":1,"id":"sample","name":name,"version_id":"1.21","source":"official"})
}
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, p: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out)
            } else {
                out.push((
                    p.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    fs::read(p).unwrap(),
                ));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}
fn fixture(root: &Path, m: &[u8]) {
    fs::create_dir_all(root.join("sample/nested")).unwrap();
    fs::write(root.join("sample/instance.json"), m).unwrap();
    fs::write(root.join("peer"), b"owned hardlink peer").unwrap();
    fs::hard_link(root.join("peer"), root.join("sample/nested/opaque")).unwrap();
    fs::create_dir(root.join("sibling")).unwrap();
    fs::write(root.join("sibling/sentinel"), b"owned sibling").unwrap();
}
#[test]
fn delete_validated_display_bytes_removes_only_owned_subtree() {
    let t = tempfile::tempdir().unwrap();
    fixture(
        t.path(),
        &serde_json::to_vec(&manifest(json!([65, 255, 66]))).unwrap(),
    );
    let result = run(false, true, t.path(), "sample");
    assert_eq!(result, (0, b"deleted\tsample\n".to_vec(), vec![]));
    assert!(!t.path().join("sample").exists());
    assert_eq!(
        fs::read(t.path().join("peer")).unwrap(),
        b"owned hardlink peer"
    );
    assert_eq!(
        fs::read(t.path().join("sibling/sentinel")).unwrap(),
        b"owned sibling"
    );
}
fn run(zig: bool, cli: bool, root: &Path, id: &str) -> (i32, Vec<u8>, Vec<u8>) {
    if zig {
        let exe = std::env::var_os("JMCL_ZIG_ORACLE").unwrap();
        let mut c = Command::new(exe);
        if cli {
            c.args([
                "instance",
                "delete",
                id,
                "--force",
                "--instances-directory",
                root.to_str().unwrap(),
            ]);
        } else {
            c.arg("rpc").stdin(Stdio::piped());
        }
        let mut child = c
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if !cli {
            writeln!(child.stdin.take().unwrap(),"{}",json!({"protocol":1,"id":"t","method":"instance.delete","params":{"directory":root,"id":id}})).unwrap();
        }
        let o = child.wait_with_output().unwrap();
        (o.status.code().unwrap(), o.stdout, o.stderr)
    } else if cli {
        let args = [
            "delete",
            id,
            "--force",
            "--instances-directory",
            root.to_str().unwrap(),
        ]
        .map(str::to_owned);
        let (mut o, mut e) = (vec![], vec![]);
        let status = instances::cli(&args, &mut o, &mut e).unwrap().unwrap();
        (status.into(), o, e)
    } else {
        let p =
            serde_json::value::RawValue::from_string(json!({"directory":root,"id":id}).to_string())
                .unwrap();
        let v = match instances::handle("instance.delete", Some(&p)).unwrap() {
            Ok(result) => json!({"protocol":1,"id":"t","event":"result","result":result}),
            Err(e) => {
                json!({"protocol":1,"id":"t","event":"error","error":{"code":e.code,"message":e.message}})
            }
        };
        let mut o = serde_json::to_vec(&v).unwrap();
        o.push(b'\n');
        (0, o, vec![])
    }
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; owned instance.delete only"]
fn display_name_delete_matches_source_and_preserves_peers() {
    let mut cases = vec![];
    for name in [
        json!("Name"),
        json!([78, 97, 109, 101]),
        json!([65, 255, 66]),
        json!([195, 169]),
        json!([0]),
        json!([10]),
        json!([256]),
        json!([1.5]),
        json!([null]),
        json!([]),
        json!(vec![65; 257]),
        Value::Null,
        json!(true),
    ] {
        cases.push((
            serde_json::to_vec(&manifest(name)).unwrap(),
            "sample",
            "normal",
        ));
    }
    for (field, value) in [
        ("schema_version", json!(2)),
        ("version_id", json!("")),
        ("source", json!("bad")),
        ("id", json!("other")),
        ("fabric_loader", json!("")),
    ] {
        let mut m = manifest(json!([255]));
        m[field] = value;
        cases.push((serde_json::to_vec(&m).unwrap(), "sample", "normal"));
    }
    for field in ["id", "name", "version_id"] {
        let mut m = manifest(json!([255]));
        m.as_object_mut().unwrap().remove(field);
        cases.push((serde_json::to_vec(&m).unwrap(), "sample", "normal"));
    }
    let m = serde_json::to_vec(&manifest(json!([255]))).unwrap();
    for id in ["bad id", "missing", ""] {
        cases.push((m.clone(), id, "normal"));
    }
    cases.push((m.clone(), "sample", "root_file"));
    cases.push((m, "bad id", "root_file"));
    let mut count = 0;
    for (m, id, state) in cases {
        for cli in [false, true] {
            let mut values = vec![];
            for zig in [true, false] {
                let t = tempfile::tempdir().unwrap();
                let root = t.path().join("instances");
                fs::create_dir(&root).unwrap();
                fixture(&root, &m);
                let before = snapshot(&root);
                let peer = root.join("peer");
                let sentinel = root.join("sibling/sentinel");
                let p = fs::metadata(&peer).unwrap();
                let s = fs::metadata(&sentinel).unwrap();
                let target = if state == "root_file" {
                    root.join("peer")
                } else {
                    root.clone()
                };
                let mut result = run(zig, cli, &target, id);
                if !cli {
                    result.1 =
                        serde_json::to_vec(&serde_json::from_slice::<Value>(&result.1).unwrap())
                            .unwrap();
                }
                let deleted = !root.join("sample").exists();
                if !deleted {
                    assert_eq!(snapshot(&root), before);
                }
                let after = fs::metadata(&peer).unwrap();
                let sa = fs::metadata(&sentinel).unwrap();
                assert_eq!(fs::read(&peer).unwrap(), b"owned hardlink peer");
                assert_eq!(fs::read(&sentinel).unwrap(), b"owned sibling");
                assert_eq!(p.modified().unwrap(), after.modified().unwrap());
                assert_eq!(s.modified().unwrap(), sa.modified().unwrap());
                assert_eq!(p.permissions().readonly(), after.permissions().readonly());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    assert_eq!(
                        (p.dev(), p.ino(), p.mode()),
                        (after.dev(), after.ino(), after.mode())
                    );
                    assert_eq!(
                        (s.dev(), s.ino(), s.mode()),
                        (sa.dev(), sa.ino(), sa.mode())
                    );
                    assert_eq!(after.nlink(), p.nlink() - u64::from(deleted));
                }
                values.push((result, deleted));
            }
            assert_eq!(
                values[0], values[1],
                "cli={cli}, id={id}, state={state}, manifest={m:?}"
            );
            count += 1;
        }
    }
    eprintln!("{count} exact delete RPC/CLI pairs; owned peer/sibling invariants preserved");
}

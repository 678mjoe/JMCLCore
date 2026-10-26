//! RPC byte targets for instance.delete only; owned opaque files.
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

fn rpc(zig: bool, p: &str) -> Value {
    let executable = if zig {
        std::env::var_os("JMCL_ZIG_ORACLE").unwrap()
    } else {
        env!("CARGO_BIN_EXE_jmcl-core-rust").into()
    };
    let mut child = Command::new(executable)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let params = if p == "ABSENT" {
        String::new()
    } else {
        format!(",\"params\":{p}")
    };
    writeln!(
        child.stdin.take().unwrap(),
        "{{\"protocol\":1,\"id\":\"t\",\"method\":\"instance.delete\"{params}}}"
    )
    .unwrap();
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success());
    assert!(o.stderr.is_empty());
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn delete_ascii_target_arrays_are_accepted() {
    let t = tempfile::tempdir().unwrap();
    fixture(
        t.path(),
        &serde_json::to_vec(&manifest(json!([255]))).unwrap(),
    );
    let p = json!({"directory":t.path().to_str().unwrap().as_bytes(),"id":b"sample"}).to_string();
    assert_eq!(rpc(false, &p)["event"], "result");
    assert!(!t.path().join("sample").exists());
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; instance.delete only"]
fn byte_target_delete_matches_source() {
    let mut count = 0;
    let mut check = |directory_form: &str,
                     id_token: &str,
                     state: &str,
                     metadata: &str,
                     extra: &str| {
        let mut values = vec![];
        for zig in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("instances");
            fs::create_dir(&root).unwrap();
            let mut m = manifest(json!([255]));
            match metadata {
                "stored_invalid_id" => m["id"] = json!("bad id"),
                "mismatch" => m["id"] = json!("other"),
                "missing_version" => {
                    m.as_object_mut().unwrap().remove("version_id");
                }
                "empty_name" => m["name"] = json!([]),
                "plain_name" => m["name"] = json!("Name"),
                _ => {}
            }
            fixture(&root, &serde_json::to_vec(&m).unwrap());
            if metadata == "missing_manifest" {
                fs::remove_file(root.join("sample/instance.json")).unwrap();
            }
            let before = snapshot(&root);
            let peer = root.join("peer");
            let sibling = root.join("sibling/sentinel");
            let pm = fs::metadata(&peer).unwrap();
            let sm = fs::metadata(&sibling).unwrap();
            let target = if state == "root_file" {
                peer.clone()
            } else if state == "missing_root" {
                temp.path().join("new-root")
            } else {
                root.clone()
            };
            let directory = match directory_form {
                "array" => json!(target.to_str().unwrap().as_bytes()).to_string(),
                "empty_array" => "[]".into(),
                "nul_array" => "[0]".into(),
                _ => json!(target.to_str().unwrap()).to_string(),
            };
            let p = match extra {
                "absent" => "ABSENT".into(),
                "null" => "null".into(),
                "array" => "[]".into(),
                "missing_id" => format!("{{\"directory\":{directory}}}"),
                "missing_directory" => format!("{{\"id\":{id_token}}}"),
                _ => format!("{{\"directory\":{directory},\"id\":{id_token}{extra}}}"),
            };
            let response = rpc(zig, &p);
            let deleted = !root.join("sample").exists();
            if !deleted {
                assert_eq!(snapshot(&root), before);
            }
            let pa = fs::metadata(&peer).unwrap();
            let sa = fs::metadata(&sibling).unwrap();
            assert_eq!(fs::read(&peer).unwrap(), b"owned hardlink peer");
            assert_eq!(fs::read(&sibling).unwrap(), b"owned sibling");
            assert_eq!(pm.modified().unwrap(), pa.modified().unwrap());
            assert_eq!(sm.modified().unwrap(), sa.modified().unwrap());
            assert_eq!(pm.permissions().readonly(), pa.permissions().readonly());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(
                    (pm.dev(), pm.ino(), pm.mode()),
                    (pa.dev(), pa.ino(), pa.mode())
                );
                assert_eq!(
                    (sm.dev(), sm.ino(), sm.mode()),
                    (sa.dev(), sa.ino(), sa.mode())
                );
                assert_eq!(pa.nlink(), pm.nlink() - u64::from(deleted));
            }
            let created = state == "missing_root" && target.is_dir();
            if created {
                assert!(fs::read_dir(&target).unwrap().next().is_none());
            }
            values.push((response, deleted, created));
        }
        assert_eq!(
            values[0], values[1],
            "directory={directory_form}, id={id_token}, state={state}, metadata={metadata}, extra={extra}"
        );
        count += 1;
    };
    for directory in ["string", "array", "empty_array", "nul_array"] {
        for id in [
            r#""sample""#,
            "[115,97,109,112,108,101]",
            "[255]",
            "[98,97,100,32,105,100]",
            "[]",
            "[109,105,115,115,105,110,103]",
        ] {
            for state in ["normal", "root_file"] {
                check(directory, id, state, "normal", "");
            }
        }
    }
    for id in [
        r#""sample""#,
        r#""bad id""#,
        r#""missing""#,
        r#""""#,
        "[115.0,97,109,112,108,101]",
        "[115.000000000000001,97,109,112,108,101]",
        r#"["115.000000000000001",97,109,112,108,101]"#,
        r#"["+1_15",97,109,112,108,101]"#,
        "[115.5]",
        "[256]",
        "[-1]",
        "[true]",
        "[null]",
        "[[115]]",
        "null",
        "{}",
    ] {
        for state in ["normal", "root_file"] {
            check("array", id, state, "plain_name", "");
        }
    }
    for metadata in [
        "stored_invalid_id",
        "mismatch",
        "missing_version",
        "empty_name",
        "missing_manifest",
    ] {
        for id in [r#""sample""#, "[115,97,109,112,108,101]"] {
            check("array", id, "normal", metadata, "");
        }
    }
    for directory in ["string", "array", "empty_array"] {
        for id in ["[115,97,109,112,108,101]", "[255]"] {
            check(directory, id, "missing_root", "normal", "");
        }
    }
    for extra in [
        "absent",
        "null",
        "array",
        "missing_id",
        "missing_directory",
        r#", "id":[115,97,109,112,108,101]"#,
        r#", "unknown":1e400"#,
        r#", "command":{},"java_paths":true"#,
    ] {
        check(
            "array",
            "[115,97,109,112,108,101]",
            "normal",
            "normal",
            extra,
        );
    }
    eprintln!(
        "{count} exact instance.delete target pairs; owned peer/sibling invariants preserved"
    );
}

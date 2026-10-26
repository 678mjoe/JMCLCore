//! Pure constrained-field rejection fidelity; no acquisition or mutation.
use jmcl_core_rust::content::modrinth as mr;
use serde_json::{Value, json};
use std::{fs, process::Command};
fn project() -> Value {
    json!({"id":"Proj1","slug":"project","project_type":"mod","title":"Title"})
}
fn version() -> Value {
    json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"url":"https://fixture.invalid/a.jar","filename":"a.jar","hashes":{"sha1":"a".repeat(40),"sha512":"metadata-only"},"size":1}],"dependencies":[{"dependency_type":"optional","file_name":"metadata-only"}]})
}
fn parse(mode: &str, bytes: &[u8]) -> Value {
    match mode {
        "project" => mr::parse_project(bytes).map(|v| serde_json::to_value(v).unwrap()),
        "version" => mr::parse_version(bytes).map(|v| serde_json::to_value(v).unwrap()),
        "versions" => mr::parse_versions(bytes).map(|v| serde_json::to_value(v).unwrap()),
        _ => panic!("pure metadata parser only"),
    }
    .unwrap_or_else(|e| json!({"error":e.0}))
}

#[test]
fn non_utf8_identifier_reaches_semantic_validation() {
    let mut v = version();
    v["id"] = json!([255]);
    assert_eq!(
        parse("version", &serde_json::to_vec(&v).unwrap()),
        json!({"error":"InvalidModrinthVersionId"})
    );
}
#[test]
#[ignore = "requires pinned JMCL_CONTENT_METADATA_ORACLE"]
fn constrained_byte_fields_match_source() {
    let oracle = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let mut count = 0;
    let mut check = |mode: &str, b: Vec<u8>| {
        fs::write(&input, &b).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&oracle)
            .arg(mode)
            .arg(&input)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(parse(mode, &b), expected, "{mode}: {:?}", b);
        assert_eq!(fs::read(&input).unwrap(), b);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
        count += 1;
    };
    for mode in ["project", "version", "versions"] {
        let fields = if mode == "project" {
            vec!["/id", "/slug", "/project_type", "/loaders/0"]
        } else {
            vec![
                "/id",
                "/project_id",
                "/version_type",
                "/game_versions/0",
                "/loaders/0",
                "/files/0/hashes/sha1",
                "/dependencies/0/version_id",
                "/dependencies/0/project_id",
            ]
        };
        for path in fields {
            for replacement in ["plain", "ascii_array", "nonutf8", "unicode"] {
                for structural in ["none", "missing", "typed"] {
                    let mut v = if mode == "project" {
                        project()
                    } else {
                        version()
                    };
                    if mode == "project" {
                        v["loaders"] = json!(["fabric"]);
                    } else {
                        v["dependencies"] = json!([{"version_id":"Dep1","project_id":"DepProj1","dependency_type":"required"}]);
                    }
                    let old = v.pointer(path).unwrap().as_str().unwrap();
                    let value = match replacement {
                        "ascii_array" => json!(old.as_bytes()),
                        "nonutf8" => json!([255]),
                        "unicode" => json!("é"),
                        _ => json!(old),
                    };
                    *v.pointer_mut(path).unwrap() = value;
                    if structural == "missing" {
                        v.as_object_mut().unwrap().remove(if mode == "project" {
                            "project_type"
                        } else {
                            "version_number"
                        });
                    }
                    if structural == "typed" {
                        v[if mode == "project" {
                            "title"
                        } else {
                            "version_number"
                        }] = json!(true);
                    }
                    check(
                        mode,
                        serde_json::to_vec(&if mode == "versions" { json!([v]) } else { v })
                            .unwrap(),
                    );
                }
            }
        }
    }
    // Complete-list typing must win over earlier item semantics.
    for field in ["version_number", "files", "dependencies"] {
        let mut first = version();
        first["id"] = json!([255]);
        let mut later = version();
        later.as_object_mut().unwrap().remove(field);
        check(
            "versions",
            serde_json::to_vec(&json!([first, later])).unwrap(),
        );
    }
    // Duplicates and nested typed errors retain typed precedence.
    for raw in [
        r#", "id":"Other""#,
        r#", "dependencies":[{"version_id":[255],"dependency_type":true}]"#,
    ] {
        let mut v = version();
        v["id"] = json!([255]);
        let b = serde_json::to_string(&v).unwrap();
        check(
            "version",
            format!("{}{raw}}}", &b[..b.len() - 1]).into_bytes(),
        );
    }
    for bad in [json!(true), json!("unknown"), json!([255])] {
        let mut v = version();
        v["id"] = json!([255]);
        v["dependencies"] = json!([{"version_id":[255],"project_id":null,"dependency_type":bad}]);
        check("version", serde_json::to_vec(&v).unwrap());
    }
    for optional in [
        Value::Null,
        json!("Dep1"),
        json!([68, 101, 112, 49]),
        json!([255]),
    ] {
        let mut v = version();
        v["dependencies"] =
            json!([{"version_id":optional,"project_id":null,"dependency_type":"required"}]);
        check("version", serde_json::to_vec(&v).unwrap());
    }
    eprintln!("{count} exact constrained byte field cases matched");
}

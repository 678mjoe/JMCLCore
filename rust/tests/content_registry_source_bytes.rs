//! Pure registry parse/validation only. No load/save, RPC, adoption or transport.
use jmcl_core_rust::content::registry;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn fixture(provider: &str) -> Value {
    let source = if provider == "manual" {
        json!({"provider":provider})
    } else {
        json!({"provider":provider,"project_id":if provider=="curseforge" {"123"} else {"Proj1"},"version_id":if provider=="curseforge" {"456"} else {"Ver1"},"slug":"project"})
    };
    json!({"schema_version":1,"entries":[{"kind":"mod","file":"mods/a.jar","sha1":"a".repeat(40),"size":1,"source":source,"enabled":true,"as_dependency":false}]})
}
fn parse(bytes: &[u8]) -> Value {
    registry::parse(bytes)
        .map(|r| serde_json::to_value(r).unwrap())
        .unwrap_or_else(|error| json!({"error":error}))
}
fn bad_field(provider: &str, field: &str, value: Value) -> Value {
    let mut v = fixture(provider);
    if field == "sha1" {
        v["entries"][0][field] = value;
    } else {
        v["entries"][0]["source"][field] = value;
    }
    v
}
#[test]
fn non_utf8_fields_reach_ordered_registry_validation() {
    for provider in ["modrinth", "curseforge", "manual"] {
        for field in ["project_id", "version_id", "slug", "sha1"] {
            let error = if field == "sha1" {
                "InvalidContentSha1"
            } else if provider == "manual" {
                "InvalidContentSource"
            } else if field == "slug" {
                "InvalidModrinthProject"
            } else if provider == "modrinth" {
                "InvalidModrinthVersionId"
            } else {
                "InvalidCurseforgeId"
            };
            for bytes in [json!([255]), json!([237, 160, 128])] {
                assert_eq!(
                    parse(&serde_json::to_vec(&bad_field(provider, field, bytes)).unwrap()),
                    json!({"error":error})
                );
            }
        }
    }
    let mut v = bad_field("modrinth", "project_id", json!([255]));
    v["entries"][0]["enabled"] = json!(1);
    assert_eq!(
        parse(&serde_json::to_vec(&v).unwrap()),
        json!({"error":"UnexpectedToken"})
    );
    v["entries"][0]["enabled"] = json!(true);
    v["schema_version"] = json!(2);
    assert_eq!(
        parse(&serde_json::to_vec(&v).unwrap()),
        json!({"error":"UnsupportedContentSchema"})
    );
}
#[test]
fn completed_registry_parse_checks_ignored_strings_before_semantics() {
    for bytes in [
        &b"{\"entries\":[],\"unknown\":\"\xff\"}"[..],
        br#"{"entries":[],"unknown":"\ud800"}"#,
        br#"{"schema_version":2,"entries":[],"unknown":"\ud800"}"#,
    ] {
        assert_eq!(parse(bytes), json!({"error":"SyntaxError"}));
    }
    assert_eq!(
        parse(br#"{"schema_version":true,"unknown":"\ud800"}"#),
        json!({"error":"UnexpectedToken"})
    );
    for unknown in ["1e400", r#""\ud83d\ude00""#, r#""\\ud800""#] {
        assert_eq!(
            parse(format!(r#"{{"entries":[],"unknown":{unknown}}}"#).as_bytes()),
            json!({"schema_version":1,"entries":[]})
        );
    }
}
fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut cases = Vec::new();
    for provider in ["modrinth", "curseforge", "manual"] {
        for field in ["project_id", "version_id", "slug", "sha1"] {
            for value in [
                json!([255]),
                json!([237, 160, 128]),
                json!([65]),
                json!(null),
                json!([]),
                json!("é"),
                json!("bad/"),
                json!([true]),
                json!([256]),
                json!([-1]),
                json!(["65", 66.0]),
                json!([65.1]),
            ] {
                let v = bad_field(provider, field, value.clone());
                cases.push((
                    format!("{provider}/{field}/{value}"),
                    serde_json::to_vec(&v).unwrap(),
                ));
            }
            let mut v = fixture(provider);
            if let Some(value) = if field == "sha1" {
                Some(&mut v["entries"][0][field])
            } else {
                v["entries"][0]["source"].get_mut(field)
            } {
                let original = value.as_str().unwrap().to_owned();
                for array in [
                    json!(original.as_bytes()),
                    json!(original.bytes().map(|n| n.to_string()).collect::<Vec<_>>()),
                ] {
                    let v = bad_field(provider, field, array);
                    cases.push((
                        format!("ascii {provider}/{field}"),
                        serde_json::to_vec(&v).unwrap(),
                    ));
                }
            }
        }
        cases.push((
            format!("plain {provider}"),
            serde_json::to_vec(&fixture(provider)).unwrap(),
        ));
    }
    for label in [
        "later-type",
        "earlier-schema",
        "earlier-file",
        "earlier-sha1",
        "earlier-size",
        "missing-next-id",
        "later-entry-enum",
        "duplicate-file",
        "duplicate-project",
        "duplicate-before-later-semantic",
    ] {
        let mut v = bad_field("modrinth", "project_id", json!([255]));
        match label {
            "later-type" => v["entries"][0]["enabled"] = json!(1),
            "earlier-schema" => v["schema_version"] = json!(2),
            "earlier-file" => v["entries"][0]["file"] = json!("../a.jar"),
            "earlier-sha1" => v["entries"][0]["sha1"] = json!("bad"),
            "earlier-size" => v["entries"][0]["size"] = json!(0),
            "missing-next-id" => {
                v["entries"][0]["source"]
                    .as_object_mut()
                    .unwrap()
                    .remove("version_id");
            }
            "later-entry-enum" => {
                let mut second = fixture("modrinth")["entries"][0].clone();
                second["kind"] = json!("invalid");
                v["entries"].as_array_mut().unwrap().push(second);
            }
            _ => {
                v = fixture("modrinth");
                let mut second = v["entries"][0].clone();
                if label == "duplicate-project" {
                    second["file"] = json!("mods/b.jar");
                }
                if label == "duplicate-before-later-semantic" {
                    second["sha1"] = json!([255]);
                }
                v["entries"].as_array_mut().unwrap().push(second);
            }
        }
        cases.push((label.into(), serde_json::to_vec(&v).unwrap()));
    }
    for provider in ["modrinth", "curseforge"] {
        for id in [
            "",
            "0",
            "0000000001",
            "4294967295",
            "4294967296",
            "12345678901",
            "AbCd0123456789AB",
            "AbCd0123456789ABCD",
        ] {
            let v = bad_field(provider, "project_id", json!(id.as_bytes()));
            cases.push((
                format!("id boundary {provider}/{id}"),
                serde_json::to_vec(&v).unwrap(),
            ));
        }
    }
    for unknown in [
        "1e400",
        r#""\ud800""#,
        r#""\udc00""#,
        r#""\ud83d\ude00""#,
        r#""\\ud800""#,
        r#"{"nested":["\ud800"]}"#,
    ] {
        for schema in ["1", "2", "true"] {
            cases.push((
                format!("unknown {schema}/{unknown}"),
                format!(r#"{{"schema_version":{schema},"entries":[],"unknown":{unknown}}}"#)
                    .into_bytes(),
            ));
        }
    }
    cases.push((
        "unknown raw FF".into(),
        b"{\"entries\":[],\"unknown\":\"\xff\"}".to_vec(),
    ));
    // Type and duplicate-field errors must precede a later semantic invalid byte.
    for raw in [
        r#"{"schema_version":true,"entries":[{"source":{"provider":"manual","slug":[255]}}]}"#,
        r#"{"schema_version":1,"schema_version":1,"entries":[{"source":{"provider":"manual","slug":[255]}}]}"#,
    ] {
        cases.push(("early typed/duplicate".into(), raw.as_bytes().to_vec()));
    }
    cases
}
fn build_oracle(exe: &Path) {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required");
    let output = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&zig).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let output = Command::new(zig)
        .args(["build-exe", "-O", "ReleaseSafe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/content_registry_strings.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            Path::new(&source).join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and Zig 0.16.0; pure parse only"]
fn registry_source_bytes_match_complete_typed_and_semantic_order() {
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join(if cfg!(windows) {
        "registry.exe"
    } else {
        "registry"
    });
    build_oracle(&exe);
    let cases = corpus();
    for (index, (name, bytes)) in cases.iter().enumerate() {
        let path = temp.path().join(format!("{index}.json"));
        fs::write(&path, bytes).unwrap();
        let before = fs::metadata(&path).unwrap();
        let output = Command::new(&exe).arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual = parse(bytes);
        assert_eq!(actual, expected, "{name}");
        assert_eq!(fs::read(&path).unwrap(), *bytes);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        assert_eq!(
            before.permissions().readonly(),
            after.permissions().readonly()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        }
    }
    eprintln!(
        "{} pure registry byte/source/order cases matched; input files unchanged; no load/save/RPC/transport",
        cases.len()
    );
}

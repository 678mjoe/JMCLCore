//! Cosmetic metadata bytes only: no operational paths, network or installation.
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
fn fields() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        ("project", project(), "/title"),
        ("version", version(), "/version_number"),
        ("version", version(), "/files/0/hashes/sha512"),
        ("version", version(), "/dependencies/0/file_name"),
    ]
}
#[test]
fn cosmetic_fields_preserve_non_utf8_without_lossy_replacement() {
    for (mode, mut value, pointer) in fields() {
        *value.pointer_mut(pointer).unwrap() = json!([65, 255, 66]);
        let result = parse(mode, &serde_json::to_vec(&value).unwrap());
        assert_eq!(
            result.pointer(pointer),
            Some(&json!([65, 255, 66])),
            "{pointer}: {result}"
        );
        *value.pointer_mut(pointer).unwrap() = json!([195, 169]);
        let result = parse(mode, &serde_json::to_vec(&value).unwrap());
        assert_eq!(
            result.pointer(pointer),
            Some(&json!("é")),
            "{pointer}: {result}"
        );
    }
}
#[test]
fn operational_fields_do_not_gain_lossy_or_raw_path_conversion() {
    for pointer in [
        "/id",
        "/project_id",
        "/files/0/filename",
        "/files/0/url",
        "/files/0/hashes/sha1",
        "/game_versions/0",
        "/loaders/0",
    ] {
        let mut value = version();
        *value.pointer_mut(pointer).unwrap() = json!([255]);
        assert_eq!(
            parse("version", &serde_json::to_vec(&value).unwrap()),
            json!({"error":"UnexpectedToken"}),
            "{pointer}"
        );
    }
}
#[test]
#[ignore = "requires pinned JMCL_CONTENT_METADATA_ORACLE"]
fn cosmetic_bytes_match_pinned_parser_and_serialization() {
    let oracle =
        std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").expect("pinned pure adapter required");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let mut count = 0;
    let mut check = |mode: &str, bytes: Vec<u8>| {
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap();
        let out = Command::new(&oracle)
            .arg(mode)
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            parse(mode, &bytes),
            expected,
            "{mode}: {}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(fs::read(&input).unwrap(), bytes);
        let after = fs::metadata(&input).unwrap();
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
        count += 1;
    };
    for (mode, value, pointer) in fields() {
        for replacement in [
            json!("plain"),
            json!("é"),
            json!([]),
            json!([65]),
            json!([195, 169]),
            json!([255]),
            json!([65, 255, 66]),
            json!([237, 160, 128]),
            json!([0]),
            Value::Null,
            json!({}),
            json!(true),
        ] {
            let mut case = value.clone();
            *case.pointer_mut(pointer).unwrap() = replacement;
            check(mode, serde_json::to_vec(&case).unwrap());
            if mode == "version" {
                check("versions", serde_json::to_vec(&json!([case])).unwrap());
            }
        }
        for token in [
            "\"65\"",
            "65.0",
            "\"+6_5\"",
            "65.0000000000000001",
            "\"65.0000000000000001\"",
            "256",
            "-1",
            "true",
            "null",
            "[]",
        ] {
            let mut case = value.clone();
            *case.pointer_mut(pointer).unwrap() = json!(["BYTE_TOKEN"]);
            let bytes = serde_json::to_string(&case)
                .unwrap()
                .replace("\"BYTE_TOKEN\"", token)
                .into_bytes();
            check(mode, bytes);
        }
        let mut absent = value.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        absent
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        check(mode, serde_json::to_vec(&absent).unwrap());
    }
    // Parsing errors keep precedence over semantic validation, as in typed Zig.
    let mut invalid = version();
    invalid["id"] = json!("bad/ref");
    invalid["version_number"] = json!([256]);
    check("version", serde_json::to_vec(&invalid).unwrap());
    eprintln!(
        "{count} exact cosmetic byte-string parser/serialization cases; inputs preserved; no runtime operations"
    );
}

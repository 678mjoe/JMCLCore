//! Completed typed Modrinth metadata only. No acquisition or executable payloads.
use jmcl_core_rust::content::modrinth as mr;
use serde_json::{Value, json};
use std::{fs, process::Command};
fn project() -> Value {
    json!({"id":"Proj1","slug":"project","project_type":"mod","title":"title"})
}
fn version() -> Value {
    json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"url":"https://fixture.invalid/a.jar","filename":"a.jar","hashes":{"sha1":"a".repeat(40)},"size":1}],"dependencies":[]})
}
fn parse(mode: &str, bytes: &[u8]) -> Value {
    match mode {
        "project" => mr::parse_project(bytes).map(|v| serde_json::to_value(v).unwrap()),
        "version" => mr::parse_version(bytes).map(|v| serde_json::to_value(v).unwrap()),
        "versions" => mr::parse_versions(bytes).map(|v| serde_json::to_value(v).unwrap()),
        _ => panic!("pure schema only"),
    }
    .unwrap_or_else(|e| json!({"error":e.0}))
}
fn unknown(value: &Value, raw: &[u8], first: bool) -> Vec<u8> {
    let bytes = serde_json::to_vec(value).unwrap();
    if first {
        [b"{\"unknown\":".as_slice(), raw, b",", &bytes[1..]].concat()
    } else {
        [&bytes[..bytes.len() - 1], b",\"unknown\":", raw, b"}"].concat()
    }
}
#[test]
fn unknown_invalid_string_bytes_are_not_silently_accepted() {
    let bytes = unknown(&project(), b"\"\xff\"", false);
    assert_eq!(parse("project", &bytes), json!({"error":"SyntaxError"}));
    let bytes = unknown(&project(), br#""\ud800""#, true);
    assert_eq!(parse("project", &bytes), json!({"error":"SyntaxError"}));
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and JMCL_CONTENT_METADATA_ORACLE"]
fn completed_typed_schema_lexical_acceptance_matches_source() {
    let source = std::env::var("JMCL_ZIG_SOURCE").unwrap();
    let head = Command::new("git")
        .args(["-C", &source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .args(["-C", &source, "diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    let oracle = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("input.json");
    let mut count = 0;
    let mut check = |mode: &str, bytes: Vec<u8>| {
        fs::write(&file, &bytes).unwrap();
        let before = fs::metadata(&file).unwrap();
        let out = Command::new(&oracle)
            .args([mode, file.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(parse(mode, &bytes), expected, "{mode}: {:?}", bytes);
        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert_eq!(
            fs::metadata(&file).unwrap().modified().unwrap(),
            before.modified().unwrap()
        );
        count += 1;
    };
    for (mode, base) in [("project", project()), ("version", version())] {
        for first in [false, true] {
            for raw in [
                b"\"\xff\"".as_slice(),
                b"[\"\xff\"]",
                b"{\"nested\":\"\xff\"}",
                b"{\"\xff\":1}",
                br#""\ud800""#,
                br#""\udc00""#,
                br#""\ud800\u0061""#,
                br#"["\ud800"]"#,
                br#""\ud83d\ude00""#,
                br#""\\ud800""#,
                br#""plain""#,
                b"\"\xc3\xa9\"",
                b"1e400",
                b"[1e400]",
            ] {
                let bytes = unknown(&base, raw, first);
                check(mode, bytes.clone());
                if mode == "version" {
                    check("versions", [b"[".as_slice(), &bytes, b"]"].concat());
                }
            }
            for invalid_id in ["bad/ref", ""] {
                let mut invalid = base.clone();
                invalid["id"] = json!(invalid_id);
                let bytes = unknown(&invalid, b"\"\xff\"", first);
                check(mode, bytes.clone());
                if mode == "version" {
                    check("versions", [b"[".as_slice(), &bytes, b"]"].concat());
                }
            }
        }
    }
    // These errors are intentionally left on the original typed parsing path.
    for bytes in [b"{\"id\":true,\"unknown\":\"\xff\"}".as_slice(),b"{\"id\":\"Proj1\",\"id\":\"\xff\"}",b"{\"id\":\"Proj1\",\"slug\":\"project\",\"project_type\":\"mod\",\"title\":[{\"unknown\":\"\xff\"}]}",b"{\"id\":\"Proj1\",\"slug\":\"project\",\"project_type\":\"mod\",\"title\":[[\"\xff\"]]}"]{check("project",bytes.to_vec());}
    eprintln!("{count} pure lexical acceptance/ordered-parser controls matched");
}

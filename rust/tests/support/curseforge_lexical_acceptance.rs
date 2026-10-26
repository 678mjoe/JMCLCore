use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn data(mode: &str) -> Value {
    if mode == "project" {
        json!({"id":42,"slug":"project","name":"Name","classId":6})
    } else {
        json!({"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"a".repeat(40)}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]})
    }
}
fn parse(mode: &str, bytes: &[u8]) -> Value {
    if mode == "project" {
        parse_project(bytes).map(|p| json!({"name":p.name}))
    } else {
        parse_file(bytes).map(|f| json!({"date":f.date,"games":f.games,"sha1":f.sha1}))
    }
    .unwrap_or_else(|e| json!({"error":e.0}))
}
fn unknown(bytes: &[u8], raw: &[u8], first: bool) -> Vec<u8> {
    if first {
        [b"{\"unknown\":".as_slice(), raw, b",", &bytes[1..]].concat()
    } else {
        [&bytes[..bytes.len() - 1], b",\"unknown\":", raw, b"}"].concat()
    }
}
fn document(mode: &str, state: &str, where_: &str, raw: &[u8]) -> Vec<u8> {
    let mut d = data(mode);
    if state == "semantic" {
        d["id"] = json!(0)
    } else if state == "typed" {
        d["id"] = json!(true)
    }
    let mut b = serde_json::to_vec(&d).unwrap();
    if where_.starts_with("inner") {
        b = unknown(&b, raw, where_.ends_with("before"));
    }
    let b = [b"{\"data\":".as_slice(), &b, b"}"].concat();
    if where_.starts_with("outer") {
        unknown(&b, raw, where_.ends_with("before"))
    } else {
        b
    }
}
#[test]
fn cf_unknown_outer_invalid_strings_are_rejected_after_complete_schema() {
    for mode in ["project", "file"] {
        for state in ["valid", "semantic"] {
            let bytes = document(mode, state, "outer_after", b"\"\xff\"");
            assert_eq!(parse(mode, &bytes), json!({"error":"SyntaxError"}));
        }
    }
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and zig; pure CF parser only"]
fn completed_cf_schema_acceptance_matches_pinned_source() {
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
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    let build = Command::new("zig")
        .args(["build-exe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/curseforge_metadata_bytes.zig",
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
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let input = temp.path().join("input.json");
    let mut count = 0;
    let mut check = |mode: &str, bytes: Vec<u8>| {
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&exe)
            .args([mode, input.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(parse(mode, &bytes), expected, "{mode}: {:?}", bytes);
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
        count += 1;
    };
    for mode in ["project", "file"] {
        for state in ["valid", "semantic"] {
            for where_ in ["outer_before", "outer_after", "inner_before", "inner_after"] {
                for raw in [
                    b"\"\xff\"".as_slice(),
                    br#""\ud800""#,
                    br#""\udc00""#,
                    br#"{"nested":["\ud800"]}"#,
                    br#""\ud83d\ude00""#,
                    br#""\\ud800""#,
                    b"\"\xc3\xa9\"",
                    b"1e400",
                ] {
                    // Literal invalid UTF8 inside RawValue never completes typed
                    // parsing; its existing diagnostic remains a separate gap.
                    if raw == b"\"\xff\"" && where_.starts_with("inner") {
                        continue;
                    }
                    check(mode, document(mode, state, where_, raw));
                }
            }
        }
        for where_ in ["outer_after", "inner_after"] {
            for raw in [b"\"\xff\"".as_slice(), br#""\ud800""#] {
                check(mode, document(mode, "typed", where_, raw));
            }
        }
    }
    eprintln!("{count} completed CF parse acceptance/ordered-error controls matched");
}

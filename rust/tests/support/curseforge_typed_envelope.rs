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

fn ordered(mode: &str, state: &str, field: &str, token: &str, first: bool) -> Vec<u8> {
    let mut d = data(mode);
    match state {
        "typed" => d["id"] = json!(true),
        "missing" => {
            d.as_object_mut().unwrap().remove(if mode == "project" {
                "slug"
            } else {
                "fileName"
            });
        }
        "semantic" => d["id"] = json!(0),
        _ => {}
    }
    let data = format!("\"data\":{}", d);
    let pagination = format!("\"pagination\":{{\"{field}\":{token}}}");
    if first {
        format!("{{{pagination},{data}}}")
    } else {
        format!("{{{data},{pagination}}}")
    }
    .into_bytes()
}
#[test]
fn data_schema_error_precedes_later_pagination_error() {
    for mode in ["project", "file"] {
        for (state, error) in [("typed", "UnexpectedToken"), ("missing", "MissingField")] {
            assert_eq!(
                parse(mode, &ordered(mode, state, "index", "1.5", false)),
                json!({"error":error})
            );
        }
    }
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and zig; pure CF parsing only"]
fn ordered_single_record_envelopes_match_source() {
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
        for state in ["valid", "typed", "missing", "semantic"] {
            for field in ["index", "pageSize", "resultCount", "totalCount"] {
                for token in [
                    "0",
                    "4294967295",
                    "4294967296",
                    "1.5",
                    "1.0000000000000001",
                    "true",
                    "null",
                    "\"x\"",
                    "\"1.0\"",
                    "\"+1_0\"",
                ] {
                    for first in [false, true] {
                        check(mode, ordered(mode, state, field, token, first));
                    }
                }
            }
        }
    }
    eprintln!("{count} exact typed single-record envelope order cases matched");
}

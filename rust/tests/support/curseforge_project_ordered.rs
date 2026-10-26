use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn parsed(bytes: &[u8]) -> Value {
    parse_project(bytes)
        .map(|p| json!({"name":p.name}))
        .unwrap_or_else(|e| json!({"error":e.0}))
}
#[test]
fn earlier_project_lexical_error_precedes_later_type() {
    assert_eq!(
        parsed(b"{\"data\":{\"unknown\":\"\xff\",\"id\":true}}"),
        json!({"error":"SyntaxError"})
    );
}
#[test]
#[ignore = "requires pinned pure CF oracle source and Zig"]
fn ordered_project_matches_pinned_source() {
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
    let mut failures = vec![];
    let mut check = |bytes: Vec<u8>| {
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&exe)
            .args(["project", input.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        let actual = parsed(&bytes);
        if actual != expected {
            failures.push(format!(
                "case{count}: Rust{actual} Zig{expected}; {:?}",
                &bytes[..bytes.len().min(400)]
            ));
        }
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
        count += 1;
    };

    let rows: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/cf_project_ordered_cases.json")).unwrap();
    assert_eq!(rows.len(), 68);
    for row in rows {
        let h = row["hex"].as_str().unwrap();
        check(
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect(),
        );
    }
    let base = json!({"data":{"id":42,"slug":"project","name":"Name","classId":6}});
    let bytes = serde_json::to_vec(&base).unwrap();
    for i in 0..bytes.len() {
        check(bytes[..i].to_vec());
    }
    for i in (0..bytes.len()).step_by(3) {
        for byte in [b'}', b'"', b',', 0xff] {
            let mut b = bytes.clone();
            b[i] = byte;
            check(b);
        }
    }
    for field in ["id", "classId"] {
        for raw in [
            "-1",
            "0",
            "4294967295",
            "4294967296",
            "1.0000000000000001",
            "1e400",
            "\"1__0\"",
            "[]",
            "{}",
            "null",
            "true",
        ] {
            let mut v = base.clone();
            v["data"][field] = json!("RAW_NUMBER");
            let b = serde_json::to_string(&v)
                .unwrap()
                .replace("\"RAW_NUMBER\"", raw);
            check(b.into_bytes());
        }
    }
    for id in [0, 42] {
        for n in [0, 1, 128, 129] {
            let mut v = base.clone();
            v["data"]["id"] = json!(id);
            v["data"]["slug"] = json!("a".repeat(n));
            check(serde_json::to_vec(&v).unwrap());
        }
    }
    for depth in [160, 512] {
        let mut b = b"{\"unknown\":".to_vec();
        b.extend(vec![b'['; depth]);
        b.extend(b"1e400");
        b.extend(vec![b']'; depth]);
        b.push(b',');
        b.extend(&bytes[1..]);
        check(b);
    }
    for n in [2 * 1024 * 1024, 2 * 1024 * 1024 + 1] {
        let mut b = bytes.clone();
        b.resize(n, b' ');
        check(b);
    }
    eprintln!(
        "{count} exact CF Project cases, {} differences",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn validated_project_slug_remains_operational_ascii() {
    let b = br#"{"data":{"id":42,"slug":[112,114,111,106,101,99,116],"name":[255],"classId":6}}"#;
    let p = parse_project(b).unwrap();
    assert_eq!(p.id, 42);
    assert_eq!(p.slug, "project");
    assert_eq!(p.name.as_bytes(), &[255]);
    assert_eq!(p.class_id, 6);
    assert!(p.available);
}

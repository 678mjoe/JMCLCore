use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn file() -> Value {
    json!({"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"a".repeat(40)}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]})
}
fn parsed(bytes: &[u8]) -> Value {
    parse_page(bytes).map(|(f,p)|json!({"ids":f.iter().map(|f|f.id).collect::<Vec<_>>(),"pagination":p.map(|p|json!({"index":p.index,"pageSize":p.page_size,"resultCount":p.result_count,"totalCount":p.total}))})).unwrap_or_else(|e|json!({"error":e.0}))
}
#[test]
fn earlier_page_lexical_failure_precedes_missing_item_fields() {
    assert_eq!(
        parsed(b"{\"data\":[{\"unknown\":\"\xff\"},{\"id\":true}]}"),
        json!({"error":"SyntaxError"})
    );
}
#[test]
#[ignore = "requires pinned pure CF oracle source and Zig"]
fn ordered_page_matches_pinned_source() {
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
    let mut check = |bytes: Vec<u8>| {
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&exe)
            .args(["page", input.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            parsed(&bytes),
            expected,
            "case {count}, prefix {:?}",
            &bytes[..bytes.len().min(1024)]
        );
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
        count += 1;
    };

    let rows: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/cf_page_ordered_cases.json")).unwrap();
    assert_eq!(rows.len(), 55);
    for row in rows {
        let h = row["hex"].as_str().unwrap();
        check(
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect(),
        );
    }
    let base = serde_json::to_vec(&json!({"data":[file()],"pagination":{"index":0}})).unwrap();
    for size in [8 * 1024 * 1024, 8 * 1024 * 1024 + 1] {
        let mut b = base.clone();
        b.resize(size, b' ');
        check(b);
    }
    for i in 0..base.len() {
        check(base[..i].to_vec());
    }
    for i in (0..base.len()).step_by(7) {
        for byte in [b'}', b'"', b',', 0xff] {
            let mut b = base.clone();
            b[i] = byte;
            check(b);
        }
    }
    for depth in [160, 512] {
        let mut b = b"{\"unknown\":".to_vec();
        b.extend(vec![b'['; depth]);
        b.extend(b"1e400");
        b.extend(vec![b']'; depth]);
        b.extend(b",\"data\":[]}");
        check(b);
    }
    for n in [50, 51] {
        for tail in [
            b"{\"id\":true}".as_slice(),
            b"{\"unknown\":\"\xff\"}",
            b"{\"id\":1,\"id\":2}",
        ] {
            let items = vec![serde_json::to_vec(&file()).unwrap(); n];
            let mut b = b"{\"data\":[".to_vec();
            for item in items {
                b.extend(item);
                b.push(b',');
            }
            b.extend(tail);
            b.extend(b"]}");
            check(b);
        }
    }
    eprintln!("{count} exact ordered CF Page cases; no resolver or network");
}

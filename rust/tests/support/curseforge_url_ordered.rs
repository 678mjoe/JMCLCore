use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn parsed(bytes: &[u8]) -> Value {
    download_url(bytes)
        .map(|u| json!(u))
        .unwrap_or_else(|e| json!({"error":e.0}))
}
#[test]
fn malformed_unknown_url_metadata_is_not_accepted() {
    assert_eq!(
        parsed(b"{\"data\":\"https://fixture.invalid/a\",\"unknown\":\"\xff\"}"),
        json!({"error":"SyntaxError"})
    );
}
#[test]
#[ignore = "requires pinned pure CF source and Zig"]
fn ordered_url_metadata_matches_source_with_named_deviation() {
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
            "-Mroot={}/tests/fixtures/curseforge_url_parser.zig",
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
    let mut deviations = 0;
    let mut check = |bytes: Vec<u8>| {
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&exe).arg(&input).output().unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        let actual = parsed(&bytes);
        if bytes == br#"{"data":[104,116,116,112,115,58,47,47,97,47,255]}"# {
            assert_eq!(
                expected,
                json!([104, 116, 116, 112, 115, 58, 47, 47, 97, 47, 255])
            );
            assert_eq!(actual, json!({"error":"UnexpectedToken"}));
            deviations += 1;
        } else if actual != expected {
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
        serde_json::from_str(include_str!("../fixtures/cf_url_ordered_cases.json")).unwrap();
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
    let base = br#"{"data":"https://fixture.invalid/a","pagination":{"index":0}}"#.to_vec();
    for i in 0..base.len() {
        check(base[..i].to_vec());
    }
    for i in (0..base.len()).step_by(3) {
        for byte in [b'}', b'"', b',', 0xff] {
            let mut b = base.clone();
            b[i] = byte;
            check(b);
        }
    }
    for raw in [
        "-1",
        "4294967295",
        "4294967296",
        "1.0000000000000001",
        "1e400",
        "\"1__0\"",
        "{}",
        "null",
        "true",
    ] {
        for first in [true, false] {
            let a = format!("\"pagination\":{{\"index\":{raw}}}");
            let b = "\"data\":null";
            check(
                if first {
                    format!("{{{a},{b}}}")
                } else {
                    format!("{{{b},{a}}}")
                }
                .into_bytes(),
            );
        }
    }
    for depth in [160, 512] {
        let mut b = b"{\"unknown\":".to_vec();
        b.extend(vec![b'['; depth]);
        b.extend(b"1e400");
        b.extend(vec![b']'; depth]);
        b.extend(b",\"data\":null}");
        check(b);
    }
    for n in [2048, 2049] {
        let mut u = "https://a/".to_owned();
        u.push_str(&"a".repeat(n - u.len()));
        check(serde_json::to_vec(&json!({"data":u})).unwrap());
    }
    for n in [2 * 1024 * 1024, 2 * 1024 * 1024 + 1] {
        let mut b = base.clone();
        b.resize(n, b' ');
        check(b);
    }
    assert_eq!(deviations, 1);
    eprintln!(
        "{} exact source-equal URL metadata cases; 1 exact COMPAT-URL-001 unsupported-byte case (not parity)",
        count - deviations
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

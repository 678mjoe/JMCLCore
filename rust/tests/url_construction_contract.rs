//! Pure URL/request construction only: never creates an HTTP Client or sends.
use serde_json::{Value, json};
use std::{collections::HashSet, fs, path::PathBuf, process::Command};
fn cases() -> Vec<Value> {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/url_construction_cases.json")).unwrap();
    assert_eq!(rows.len(), 32);
    let names: HashSet<_> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), rows.len());
    for (scope, expected) in [
        (
            "COMPAT-URL-001",
            &[
                "unicode_path",
                "dot_segments",
                "escaped_dot",
                "raw_ff_path",
                "raw_ff_query",
                "surrogate_path",
            ][..],
        ),
        (
            "strict-target-control",
            &[
                "ascii",
                "escaped_ff",
                "escaped_unicode",
                "escaped_slash",
                "bad_percent",
                "empty_path",
                "query_only",
                "fragment",
            ][..],
        ),
    ] {
        let actual: HashSet<_> = rows
            .iter()
            .filter(|r| r["scope"] == scope)
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            actual,
            expected.iter().copied().collect(),
            "named scope {scope}"
        );
    }

    rows
}
fn input(row: &Value) -> Vec<u8> {
    row["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn construct(bytes: &[u8]) -> Value {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return json!({"boundary_error":"NonUtf8String"}),
    };
    let url = match reqwest::Url::parse(text) {
        Ok(url) => url,
        Err(error) => return json!({"parse_error":error.to_string()}),
    };
    // Request::new is an inert data constructor, not Client::get/send/build.
    let request = reqwest::blocking::Request::new(reqwest::Method::GET, url);
    let url = request.url();
    let target = format!(
        "{}{}",
        url.path(),
        url.query().map(|q| format!("?{q}")).unwrap_or_default()
    );
    json!({"scheme":url.scheme(),"target":target,"host":url.host_str(),"url":url.as_str()})
}
#[test]
fn accepted_url_deviation_has_exact_named_construction_results() {
    let mut deviations = 0;
    let mut strict = 0;
    for row in cases() {
        assert_eq!(construct(&input(&row)), row["rust"], "{}", row["name"]);
        match row["scope"].as_str().unwrap() {
            "strict-target-control" => {
                assert_eq!(
                    row["zig"]["target"], row["rust"]["target"],
                    "{}",
                    row["name"]
                );
                strict += 1;
            }
            "COMPAT-URL-001" => {
                assert_ne!(
                    row["zig"]["target"], row["rust"]["target"],
                    "{}",
                    row["name"]
                );
                deviations += 1;
            }
            "construction-only-diagnostic" => {}
            other => panic!("unknown fixture scope {other}"),
        }
    }
    assert_eq!((deviations, strict), (6, 8));
    eprintln!(
        "32 exact Rust URL construction cases; COMPAT-URL-001: 6 named deviations, 8 strict target controls; no HTTP client/network"
    );
}
#[test]
#[ignore = "requires Zig 0.16.0 for a pure std.Uri fixture"]
fn pinned_source_url_construction_matches_exact_bytes() {
    let compiler = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&compiler).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join(if cfg!(windows) {
        "url-construction.exe"
    } else {
        "url-construction"
    });
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/url_construction.zig");
    let output = Command::new(&compiler)
        .arg("build-exe")
        .arg(&fixture)
        .args(["-O", "ReleaseSafe"])
        .arg(format!("-femit-bin={}", executable.display()))
        .arg("--cache-dir")
        .arg(temp.path().join("cache"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pure fixture build: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for row in cases() {
        let bytes = input(&row);
        let path = temp.path().join("input.bin");
        fs::write(&path, &bytes).unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let output = Command::new(&executable).arg(&path).output().unwrap();
        assert!(output.status.success(), "{}", row["name"]);
        assert!(output.stderr.is_empty());
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(source, row["zig"], "source {}", row["name"]);
        assert_eq!(construct(&bytes), row["rust"], "Rust {}", row["name"]);
        if row["scope"] == "strict-target-control" {
            assert_eq!(
                source["target"],
                construct(&bytes)["target"],
                "{}",
                row["name"]
            );
        }
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }
    eprintln!(
        "32 exact pinned std.Uri/Rust construction pairs; COMPAT-URL-001 explicitly not wire parity; input files unchanged"
    );
}

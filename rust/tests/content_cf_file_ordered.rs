//! Pure File envelope parser; no resolver, transport or artifact operations.
use jmcl_core_rust::content::curseforge as cf;
use serde_json::{Value, json};
use std::{fs, process::Command};
fn parse(b: &[u8]) -> Value {
    cf::parse_file(b).map(|f|json!({"id":f.id,"project":f.project,"name":f.name,"sha1":f.sha1,"size":f.size,"url":f.url})).unwrap_or_else(|e|json!({"error":e.0}))
}
fn base() -> Value {
    json!({"data":{"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"a".repeat(40)}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1"]}})
}
fn field(v: &Value, key: &str, raw: &[u8]) -> Vec<u8> {
    let mut v = v.clone();
    v.as_object_mut().unwrap().remove(key);
    let b = serde_json::to_vec(&v).unwrap();
    [b"{\"", key.as_bytes(), b"\":", raw, b",", &b[1..]].concat()
}
#[test]
fn cf_early_lexical_error_precedes_later_typed_failure() {
    assert_eq!(
        parse(b"{\"data\":{\"unknown\":\"\xff\",\"id\":true}}"),
        json!({"error":"SyntaxError"})
    );
}
#[test]
#[ignore = "requires pinned pure metadata oracle"]
fn cf_file_ordered_source_corpus() {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").unwrap();
    let head = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    let oracle = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("input.json");
    let (mut n, mut failures) = (0, vec![]);
    let mut check = |name: &str, b: Vec<u8>| {
        fs::write(&path, &b).unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let o = Command::new(&oracle)
            .arg("cffile")
            .arg(&path)
            .output()
            .unwrap();
        assert!(o.status.success());
        assert!(o.stderr.is_empty());
        let z: Value = serde_json::from_slice(&o.stdout).unwrap();
        let r = parse(&b);
        if r != z {
            failures.push(format!(
                "{name}: Rust{r} Zig{z}; {:?}",
                &b[..b.len().min(400)]
            ));
        }
        assert_eq!(fs::read(&path).unwrap(), b);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        n += 1;
    };
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/cf_file_ordered_cases.json")).unwrap();
    assert_eq!(rows.len(), 41);
    for row in rows {
        let h = row["hex"].as_str().unwrap();
        check(
            row["name"].as_str().unwrap(),
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect(),
        );
    }
    let v = base();
    for key in ["id", "modId", "releaseType", "fileLength"] {
        for raw in [
            "0",
            "-0",
            "-1",
            "255",
            "256",
            "4294967295",
            "4294967296",
            "18446744073709551615",
            "18446744073709551616",
            "1.0000000000000001",
            "1e400",
            "\"+1\"",
            "\"1__0\"",
            "\"-0\"",
            "\"1.5\"",
            "true",
            "null",
            "[]",
            "{}",
            "[\"\\ud800\"]",
        ] {
            check(
                &format!("number-{key}-{raw}"),
                [
                    b"{\"data\":".as_slice(),
                    &field(&v["data"], key, raw.as_bytes()),
                    b"}",
                ]
                .concat(),
            );
        }
    }
    for key in [
        "id",
        "modId",
        "isAvailable",
        "fileName",
        "releaseType",
        "hashes",
        "fileDate",
        "fileLength",
        "downloadUrl",
        "gameVersions",
        "dependencies",
    ] {
        for raw in ["null", "true", "{}", "[]", "\"x\"", "7", "}", "]"] {
            check(
                &format!("type-{key}-{raw}"),
                [
                    b"{\"data\":".as_slice(),
                    &field(&v["data"], key, raw.as_bytes()),
                    b"}",
                ]
                .concat(),
            );
        }
    }
    for raw in [
        "-1",
        "255",
        "256",
        "4294967295",
        "4294967296",
        "1.5",
        "1.0000000000000001",
        "\"1__0\"",
        "{}",
        "null",
    ] {
        for (field_name, nested) in [
            (
                "hashes",
                format!("[{{\"value\":\"{}\",\"algo\":{raw}}}]", "a".repeat(40)),
            ),
            (
                "dependencies",
                format!("[{{\"modId\":{raw},\"relationType\":3}}]"),
            ),
            (
                "dependencies",
                format!("[{{\"modId\":42,\"relationType\":{raw}}}]"),
            ),
        ] {
            check(
                "nested-number",
                [
                    b"{\"data\":".as_slice(),
                    &field(&v["data"], field_name, nested.as_bytes()),
                    b"}",
                ]
                .concat(),
            );
        }
    }
    for raw in [
        "null",
        "{}",
        "{\"index\":1e400}",
        "{\"pageSize\":\"1__0\"}",
        "{\"resultCount\":-1}",
        "{\"totalCount\":4294967296}",
        "{\"index\":1,\"index\":\"\\ud800\"}",
        "{\"unknown\":\"\\ud800\",\"index\":true}",
    ] {
        check("pagination", field(&v, "pagination", raw.as_bytes()));
    }
    for key in ["hashes", "dependencies", "gameVersions"] {
        for raw in [
            "[{}]",
            "[null]",
            "[true]",
            "[1]",
            "[{},]",
            "[{\"unknown\":\"\\ud800\"}]",
            "[\"\\ud800\"]",
        ] {
            check(
                "nested",
                [
                    b"{\"data\":".as_slice(),
                    &field(&v["data"], key, raw.as_bytes()),
                    b"}",
                ]
                .concat(),
            );
        }
    }
    for s in [
        b"{\"data\":{},\"data\" \"\\ud800\"}".as_slice(),
        b"{\"pagination\":{},\"pagination\":\"\\ud800\"}",
        b"{} true",
        b"{\"data\":{}} true",
        b"{\"data\":{\"id\":1,\"id\":\"\\ud800\"}}",
    ] {
        check("duplicate-missing", s.to_vec());
    }
    for depth in [160, 512] {
        let b = [b"[".repeat(depth), b"1e400".to_vec(), b"]".repeat(depth)].concat();
        check("deep-unknown", field(&v, "unknown", &b));
    }
    let bytes = serde_json::to_vec(&v).unwrap();
    for i in 0..bytes.len() {
        check("truncated", bytes[..i].to_vec());
    }
    for i in (0..bytes.len()).step_by(7) {
        for c in [b'}', b'"', b',', 0xff] {
            let mut b = bytes.clone();
            b[i] = c;
            check("mutation", b);
        }
    }
    for size in [2 * 1024 * 1024, 2 * 1024 * 1024 + 1] {
        let mut b = bytes.clone();
        b.resize(size, b' ');
        check("body-limit", b);
    }
    eprintln!("{n} pure CF File ordered cases; no resolver or network");
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

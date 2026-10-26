//! Project-only pure parser oracle. No service, filesystem mutation API or transport.
use jmcl_core_rust::content::modrinth::parse_project;
use serde_json::{Value, json};
use std::{fs, process::Command};
fn actual(b: &[u8]) -> Value {
    parse_project(b)
        .map(|p| serde_json::to_value(p).unwrap())
        .unwrap_or_else(|e| json!({"error":e.0}))
}
#[test]
fn project_known_lexical_failure_precedes_missing_fields() {
    assert_eq!(actual(b"{\"title\":\"\xff"), json!({"error":"SyntaxError"}));
    assert_eq!(
        actual(b"{\"unknown\":\"\\ud800\"}"),
        json!({"error":"SyntaxError"})
    );
    assert_eq!(
        actual(b"{\"title\":[{\"bad\":\"\xff\"}]}"),
        json!({"error":"UnexpectedToken"})
    );
}
#[test]
#[ignore = "requires pinned pure metadata oracle"]
fn project_ordered_source_corpus() {
    let oracle = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let source = std::env::var_os("JMCL_ZIG_SOURCE").unwrap();
    assert_eq!(
        String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(source)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout
        )
        .unwrap()
        .trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("input.json");
    let mut count = 0;
    let mut failures = vec![];
    let mut check = |label: &str, b: Vec<u8>| {
        fs::write(&path, &b).unwrap();
        let old = fs::metadata(&path).unwrap().modified().unwrap();
        let out = Command::new(&oracle)
            .arg("project")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "oracle failure {label}: {:?}",
            out.stderr
        );
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        let got = actual(&b);
        if got != expected {
            failures.push(format!("{label}: {b:?}: Rust {got} Zig {expected}"));
        }
        assert_eq!(fs::read(&path).unwrap(), b);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), old);
        count += 1;
    };
    let prefix: &[u8] = b"{\"id\":\"Proj1\",\"slug\":\"project\",\"project_type\":\"mod\",";
    let values: Vec<&[u8]> = vec![
        b"\"ok\"",
        b"\"\xc3\xa9\"",
        b"\"\xff\"",
        b"\"\xff",
        b"\"\xc3",
        b"\"\xc3\"",
        br#""\ud800""#,
        br#""\ud800"#,
        br#""\udc00""#,
        br#""\ud800\u0041""#,
        br#""\ud83d\ude00""#,
        br#""\\ud800""#,
        br#""\q""#,
        b"\"\n\"",
        b"[]",
        b"[65]",
        b"[65}",
        b"[[65}]",
        b"[\"65\"]",
        b"[65.0]",
        b"[65.00000000000000001]",
        b"[256]",
        b"[-1]",
        b"[256x]",
        b"[01]",
        b"[1e]",
        b"[1e400]",
        b"[true]",
        b"[null]",
        b"[{}]",
        b"[[\"\xff\"]]",
        b"[{\"bad\":\"\xff\"}]",
        b"[65,]",
        b"[65,,66]",
        b"null",
        b"false",
        b"1e400",
        b"{}",
        b"{\"a\":\"\xff\"}",
        b"[\"\xff\",",
        b"{\"a\":\"\xff",
        b"{\"a\":1,}",
        b"[1,]",
        b"\"",
        b"",
        b"tru",
        b"truex",
    ];
    for key in ["title", "id", "loaders", "unknown"] {
        for value in &values {
            let field = [b"\"".as_slice(), key.as_bytes(), b"\":", value].concat();
            check("field complete", [prefix, &field, b"}"].concat());
            check(
                "field before type",
                [b"{".as_slice(), &field, b",\"id\":true}"].concat(),
            );
            check(
                "field after type",
                [b"{\"id\":true,".as_slice(), &field, b"}"].concat(),
            );
        }
    }
    for bytes in [
        b"".as_slice(),
        b" ",
        b"{",
        b"{}",
        b"[]",
        b"null",
        b"\"\xff\"",
        b"{\"id\":\"x\",\"id\"",
        b"{\"id\":\"x\",\"id\" false}",
        b"{\"id\":\"x\",\"id\":\"\xff\"}",
        b"{} x",
        b"{\"title\":null,}",
        b"{\"id\":\"x\" \"slug\":\"y\"}",
    ] {
        check("root/order", bytes.to_vec());
    }
    for raw in [
        b"{1e".as_slice(),
        b"{tru",
        b"{[\"\xff",
        b"{\"id\":\"x\",1e",
        b"{\"id\":\"x\",tru",
        b"{\"id\":\"x\",[\"\xff",
    ] {
        check("key token boundary", raw.to_vec());
    }
    for depth in [0, 1, 16, 160, 512] {
        for scalar in [b"1e400".as_slice(), b"\"\xff\"", br#""\ud800""#] {
            let mut b = prefix.to_vec();
            b.extend_from_slice(b"\"unknown\":");
            b.extend(std::iter::repeat_n(b'[', depth));
            b.extend_from_slice(scalar);
            b.extend(std::iter::repeat_n(b']', depth));
            b.push(b'}');
            check("deep unknown", b);
        }
    }
    // Deterministic bounded punctuation/lexical mutations, not random fuzzing.
    let valid = br#"{"id":"Proj1","slug":"project","project_type":"mod","title":[65],"loaders":["fabric"]}"#;
    for tail in [
        b"\"".as_slice(),
        b"-",
        b"tru",
        b"{}",
        b"[]",
        b"1",
        b"\"\xff",
        b" true",
    ] {
        check("trailing complete", [valid.as_slice(), tail].concat());
        check("trailing missing", [b"{}".as_slice(), tail].concat());
    }
    for end in 0..valid.len() {
        check("prefix truncation", valid[..end].to_vec());
    }
    for at in 1..valid.len() - 1 {
        for replacement in [b' ', b'"', b':', b',', b'\\', 0xff] {
            let mut b = valid.to_vec();
            b[at] = replacement;
            check("single byte mutation", b);
        }
    }
    for raw in [
        br#"{"id":"Proj1","slug":"project","project_type":"mod","unknown":1,"unknown":2}"#
            .as_slice(),
        br#"{"\u0069d":"Proj1","id":"again"}"#,
        br#"{"id":"Proj1","slug":"project","project_type":"mod","title":null,"title":null}"#,
    ] {
        check("escaped/duplicate key", raw.to_vec());
    }
    for depth in [160, 512] {
        let mut b = prefix.to_vec();
        b.extend_from_slice(b"\"unknown\":");
        for _ in 0..depth {
            b.extend_from_slice(b"{\"k\":[");
        }
        b.extend_from_slice(b"1e400");
        for _ in 0..depth {
            b.extend_from_slice(b"]}");
        }
        b.push(b'}');
        check("alternating deep containers", b);
    }
    let unknown = br#"{"k":["text",1,true,null,{"v":2}]}"#;
    for at in 1..unknown.len() - 1 {
        for replacement in [b'"', b':', b',', b'}', b']', 0xff] {
            let mut v = unknown.to_vec();
            v[at] = replacement;
            check(
                "unknown subtree mutation",
                [prefix, b"\"unknown\":", &v, b"}"].concat(),
            );
        }
    }
    for v in [
        b"[1}".as_slice(),
        br#"["x"}"#,
        br#"{"k":1]"#,
        br#"{"k":[1}"#,
        b"[1]]",
        br#"{"k":"x"""#,
        b"{\"k\":1,\"\xff\"",
    ] {
        check(
            "unknown boundaries",
            [prefix, b"\"unknown\":", v, b"}"].concat(),
        );
    }
    let mut bounded = prefix.to_vec();
    bounded.extend_from_slice(b"\"unknown\":\"");
    bounded.resize(1024 * 1024 - 2, b'a');
    bounded.extend_from_slice(b"\"}");
    check("at size limit", bounded);
    let mut big = vec![b' '; 1024 * 1024 + 1];
    big[0] = b'{';
    check("size first", big);
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("{count} ordered Project cases matched");
}

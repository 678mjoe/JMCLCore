//! Pure ordered Version parser cases; no resolver, artifact or runtime operations.
use jmcl_core_rust::content::modrinth as mr;
use serde_json::{Value, json};
use std::{fs, process::Command};
fn base() -> Value {
    json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"url":"https://fixture.invalid/a.jar","filename":"a.jar","primary":false,"hashes":{"sha1":"a".repeat(40)},"size":7}],"dependencies":[]})
}
fn parse(mode: &str, b: &[u8]) -> Value {
    match mode {
        "version" => mr::parse_version(b).map(|v| serde_json::to_value(v).unwrap()),
        "versions" => mr::parse_versions(b).map(|v| serde_json::to_value(v).unwrap()),
        _ => panic!("pure parser only"),
    }
    .unwrap_or_else(|e| json!({"error":e.0}))
}
fn raw_field(v: &Value, key: &str, raw: &[u8]) -> Vec<u8> {
    let mut v = v.clone();
    v.as_object_mut().unwrap().remove(key);
    let bytes = serde_json::to_vec(&v).unwrap();
    [
        b"{\"".as_slice(),
        key.as_bytes(),
        b"\":",
        raw,
        b",",
        &bytes[1..],
    ]
    .concat()
}
#[test]
fn earlier_version_lexical_failure_is_not_displaced_by_later_type() {
    assert_eq!(
        parse("version", b"{\"unknown\":\"\xff\",\"id\":true}"),
        json!({"error":"SyntaxError"})
    );
}
#[test]
fn dependency_enum_retains_named_and_integer_forms() {
    for (raw, name) in [
        (b"0".as_slice(), "required"),
        (b"3", "embedded"),
        (br#""+1""#, "optional"),
        (br#""2""#, "incompatible"),
    ] {
        let mut v = base();
        v["dependencies"] = json!([{"dependency_type":"placeholder"}]);
        let b = serde_json::to_vec(&v).unwrap();
        let b = String::from_utf8(b)
            .unwrap()
            .replace("\"placeholder\"", std::str::from_utf8(raw).unwrap());
        assert_eq!(
            parse("version", b.as_bytes())["dependencies"][0]["dependency_type"],
            name
        );
    }
}
#[test]
#[ignore = "requires pinned pure metadata oracle"]
fn ordered_version_and_array_source_corpus() {
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
    let oracle = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("input.json");
    let (mut count, mut failures) = (0, vec![]);
    let mut check = |label: &str, mode: &str, b: Vec<u8>| {
        fs::write(&path, &b).unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let output = Command::new(&oracle).arg(mode).arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "oracle {label}: {:?}",
            output.stderr
        );
        assert!(output.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual = parse(mode, &b);
        if actual != expected {
            failures.push(format!(
                "{label}/{mode} {}bytes {:?}: Rust {actual} Zig {expected}",
                b.len(),
                &b[..b.len().min(600)]
            ));
        }
        assert_eq!(fs::read(&path).unwrap(), b);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        count += 1;
    };
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/version_ordered_cases.json")).unwrap();
    assert_eq!(rows.len(), 104);
    for row in rows {
        let h = row["hex"].as_str().unwrap();
        let b = (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect();
        check(
            row["name"].as_str().unwrap(),
            row["mode"].as_str().unwrap(),
            b,
        );
    }
    let tokens = [
        "0",
        "1",
        "2",
        "3",
        "4",
        "-1",
        "-0",
        "0.0",
        "1e0",
        "true",
        "null",
        "[]",
        "{}",
        "\"required\"",
        "\"embedded\"",
        "\"REQUIRED\"",
        "\"+0\"",
        "\"-0\"",
        "\"1__0\"",
        "\"0__1\"",
        "\"0.0\"",
        "\"0e0\"",
        "\"4\"",
        "\"_1\"",
        "\"1_\"",
        "\"0x1\"",
        "\"\"",
        "[114,101,113,117,105,114,101,100]",
    ];
    for raw in tokens {
        let dependency = [b"{\"dependency_type\":".as_slice(), raw.as_bytes(), b"}"].concat();
        let b = raw_field(
            &base(),
            "dependencies",
            &[b"[".as_slice(), &dependency, b"]"].concat(),
        );
        check("dependency enum", "version", b.clone());
        check(
            "dependency enum array",
            "versions",
            [b"[".as_slice(), &b, b"]"].concat(),
        );
    }
    for raw in [
        b"7".as_slice(),
        b"7.0",
        b"7.0000000000000001",
        br#""7.0000000000000001""#,
        b"18446744073709551615",
        br#""18446744073709551615.0""#,
        b"18446744073709551616",
        b"-1",
        b"true",
        b"{\"bad\":\"\xff\"}",
        b"[\"\xff\"]",
        b"\"\xff\"",
        b"\"\xff",
    ] {
        let file = raw_field(&base()["files"][0], "size", raw);
        check(
            "typed u64",
            "version",
            raw_field(&base(), "files", &[b"[".as_slice(), &file, b"]"].concat()),
        );
    }
    for key in ["files", "dependencies", "game_versions", "loaders"] {
        for raw in [
            b"[]".as_slice(),
            b"[true]",
            b"[\"\xff\"]",
            b"[[\"\xff\"]]",
            b"[{\"unknown\":\"\xff\"}]",
            b"[{},]",
            b"[{}]",
            b"[1}",
            b"{",
            b"\"\xff\"",
        ] {
            check("slice boundary", "version", raw_field(&base(), key, raw));
        }
    }
    for depth in [160, 512] {
        let mut unknown = vec![b'['; depth];
        unknown.extend_from_slice(b"1e400");
        unknown.extend(vec![b']'; depth]);
        check(
            "deep unknown",
            "version",
            raw_field(&base(), "unknown", &unknown),
        );
    }
    for n in [512, 513] {
        let mut values = vec![base(); n];
        check(
            "array count",
            "versions",
            serde_json::to_vec(&values).unwrap(),
        );
        values[0]["id"] = json!("bad/ref");
        check(
            "count before semantics",
            "versions",
            serde_json::to_vec(&values).unwrap(),
        );
        values[n - 1].as_object_mut().unwrap().remove("files");
        check(
            "whole typed before count",
            "versions",
            serde_json::to_vec(&values).unwrap(),
        );
    }
    for field in ["primary", "size", "hashes"] {
        for raw in [b"}".as_slice(), b"]", b",", b":"] {
            let file = raw_field(&base()["files"][0], field, raw);
            check(
                "value punctuation",
                "version",
                raw_field(&base(), "files", &[b"[".as_slice(), &file, b"]"].concat()),
            );
        }
    }
    for raw in [b"}".as_slice(), b"]", b",", b":"] {
        check("root punctuation", "version", raw.to_vec());
        let dep = [b"{\"dependency_type\":".as_slice(), raw, b"}"].concat();
        check(
            "enum punctuation",
            "version",
            raw_field(
                &base(),
                "dependencies",
                &[b"[".as_slice(), &dep, b"]"].concat(),
            ),
        );
    }
    let valid = serde_json::to_vec(&base()).unwrap();
    for at in (1..valid.len() - 1).step_by(5) {
        for replacement in [b'"', b':', b',', 0xff] {
            let mut b = valid.clone();
            b[at] = replacement;
            check("bounded token mutation", "version", b.clone());
            check(
                "bounded token mutation array",
                "versions",
                [b"[".as_slice(), &b, b"]"].concat(),
            );
        }
    }
    for end in (0..valid.len()).step_by(7) {
        check("bounded truncation", "version", valid[..end].to_vec());
    }
    for (field, limit, value) in [
        ("files", 64, base()["files"][0].clone()),
        ("dependencies", 128, json!({"dependency_type":"required"})),
        ("game_versions", 128, json!("1.20.1")),
        ("loaders", 32, json!("fabric")),
    ] {
        for n in [limit, limit + 1] {
            let mut v = base();
            v[field] = json!(vec![value.clone(); n]);
            check(
                "semantic length",
                "version",
                serde_json::to_vec(&v).unwrap(),
            );
        }
    }
    for field in ["id", "files", "dependencies"] {
        let value = base()[field].clone();
        let raw = serde_json::to_vec(&value).unwrap();
        let duplicate = [
            b"{\"".as_slice(),
            field.as_bytes(),
            b"\":",
            &raw,
            b",\"",
            field.as_bytes(),
            b"\":\"\xff\"}",
        ]
        .concat();
        check("duplicate before malformed value", "version", duplicate);
    }
    for (mode, limit) in [("version", 4 * 1024 * 1024), ("versions", 16 * 1024 * 1024)] {
        let mut v = base();
        v["unknown"] = json!("a".repeat(limit - 1024));
        let b = serde_json::to_vec(&if mode == "versions" { json!([v]) } else { v }).unwrap();
        check("near size", mode, b.clone());
        let mut exact = b;
        exact.resize(limit, b' ');
        check("exact size", mode, exact);
        check("over size", mode, vec![b' '; limit + 1]);
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("{count} ordered Version/versions source cases matched");
}

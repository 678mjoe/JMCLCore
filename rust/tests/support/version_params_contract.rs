//! Actual private parameter helpers only. Never call handle/handle_with/Service.
use super::*;
use std::{fs, process::Command};
fn actual(method: &str, bytes: &[u8]) -> Value {
    let raw = serde_json::from_slice::<Box<RawValue>>(bytes).unwrap();
    let parsed = match parse_params(Some(&raw)) {
        Ok(v) => v,
        Err(_) => return serde_json::json!({"accepted":false}),
    };
    let canonical = match canonical_params_view(method, parsed.as_ref()) {
        Ok(value) => value,
        Err(_) => return serde_json::json!({"accepted":false}),
    };
    let parsed = canonical.as_deref();
    if validate_params(method, parsed).is_err() {
        return serde_json::json!({"accepted":false});
    }
    let p = parsed.unwrap().as_object().unwrap();
    let mut result = serde_json::json!({"accepted":true,"source":source(p).unwrap()});
    let r = result.as_object_mut().unwrap();
    if method == "version.list" {
        r.insert("type".into(), p.get("type").cloned().unwrap_or(Value::Null));
        r.insert(
            "limit".into(),
            serde_json::json!(
                p.get("limit")
                    .map(|v| metadata::unsigned(v, u64::MAX).unwrap())
                    .unwrap_or(20)
            ),
        );
    } else {
        r.insert("id".into(), p["id"].clone());
    }
    if matches!(method, "version.resolve" | "install.plan") {
        let _actual_loader_count = loader_count(p).unwrap();
        for key in ["fabric_loader", "neoforge_version", "forge_version"] {
            r.insert(key.into(), p.get(key).cloned().unwrap_or(Value::Null));
        }
    }
    if method == "install.plan" {
        for key in ["os", "arch"] {
            r.insert(key.into(), p.get(key).cloned().unwrap_or(Value::Null));
        }
    }
    result
}
fn assert_source_schemas(source: &std::path::Path) {
    let adapter = include_str!("../fixtures/version_params.zig");
    fn compact(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }
    fn schema(text: &str, name: &str) -> String {
        compact(
            text.split_once(&format!("const {name} = struct {{"))
                .expect("schema start")
                .1
                .split_once("};")
                .expect("schema end")
                .0,
        )
        .trim_end_matches(',')
        .to_owned()
    }
    fn options(text: &str, start: &str) -> String {
        compact(
            text.split_once(start)
                .expect("parse call")
                .1
                .split_once(".{")
                .expect("parse options")
                .1
                .split_once("}")
                .expect("options end")
                .0,
        )
        .trim_end_matches(',')
        .to_owned()
    }
    for (path, original, mirrored) in [
        ("version.zig", "VersionListParams", "List"),
        ("version.zig", "VersionGetParams", "Get"),
        ("version.zig", "VersionResolveParams", "Resolve"),
        ("install.zig", "InstallPlanParams", "Plan"),
    ] {
        let text = fs::read_to_string(source.join("src/protocol").join(path)).unwrap();
        let expected = schema(&text, original).replace(
            "version_manifest.VersionType",
            "core.version_manifest.VersionType",
        );
        assert_eq!(
            schema(adapter, mirrored),
            expected,
            "exact pinned schema/default drift: {original}"
        );
        assert_eq!(
            options(&text, &format!("std.json.parseFromValue({original},")),
            ".ignore_unknown_fields=true"
        );
    }
    assert_eq!(
        options(adapter, "std.json.parseFromValue(T,"),
        ".ignore_unknown_fields=true"
    );
    let protocol = fs::read_to_string(source.join("src/protocol/protocol.zig")).unwrap();
    assert!(compact(&protocol).contains("params:?std.json.Value=null"));
    assert_eq!(
        options(&protocol, "std.json.parseFromSlice(Request,"),
        ".ignore_unknown_fields=true"
    );
    assert_eq!(
        options(adapter, "std.json.parseFromSlice(std.json.Value,"),
        ".ignore_unknown_fields=true"
    );
}
#[test]
#[ignore = "requires pinned Zig schema adapter; pure request-helper acceptance/value proof only"]
fn version_parameter_utf8_bytes_match_pinned_dynamic_schema() {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required");
    assert_eq!(
        String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout
        )
        .unwrap()
        .trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    assert_source_schemas(std::path::Path::new(&source));
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join(if cfg!(windows) {
        "schema.exe"
    } else {
        "schema"
    });
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&zig).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let output = Command::new(zig)
        .args(["build-exe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/version_params.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            std::path::Path::new(&source).join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut cases = vec![];
    let mut add = |method: &str, label: &str, key: &str, value: Value| {
        let mut p = if method == "version.list" {
            serde_json::json!({})
        } else {
            serde_json::json!({"id":"demo"})
        };
        if !key.is_empty() {
            p.as_object_mut().unwrap().insert(key.into(), value);
        }
        let mut bytes = serde_json::to_vec(&p).unwrap();
        if label == "source--0" {
            // Keep the original dynamic negative-zero token, not serde's -0.0 spelling.
            bytes = String::from_utf8(bytes)
                .unwrap()
                .replace(":-0.0", ":-0")
                .into_bytes();
        }
        cases.push((method.to_owned(), label.to_owned(), bytes));
    };
    for method in [
        "version.list",
        "version.get",
        "version.resolve",
        "install.plan",
    ] {
        add(method, "default", "", Value::Null);
        for token in [
            r#""official""#,
            r#""bmclapi""#,
            "0",
            "1",
            "2",
            "-1",
            "0.0",
            "-0",
            r#""0""#,
            r#""+1""#,
            r#""0_1""#,
            r#""-00""#,
            r#""-0""#,
            r#""0.0""#,
            "null",
            "[]",
        ] {
            add(
                method,
                &format!("source-{token}"),
                "source",
                serde_json::from_str(token).unwrap(),
            );
        }
    }
    for (method, key, tokens) in [
        (
            "version.list",
            "type",
            &[r#""release""#, "0", "3", "4", "0.0", r#""+1""#, "null"][..],
        ),
        (
            "install.plan",
            "os",
            &[
                r#""windows""#,
                "0",
                "2",
                "3",
                r#""1""#,
                "null",
                r#""amd64""#,
            ][..],
        ),
        (
            "install.plan",
            "arch",
            &[r#""x86""#, "0", "3", "4", r#""1""#, "null", r#""amd64""#][..],
        ),
    ] {
        for token in tokens {
            add(
                method,
                &format!("{key}-{token}"),
                key,
                serde_json::from_str(token).unwrap(),
            );
        }
    }
    for method in ["version.get", "version.resolve", "install.plan"] {
        for value in [
            serde_json::json!([100, 101, 109, 111]),
            serde_json::json!("版本"),
            serde_json::json!([255]),
        ] {
            add(method, &format!("id-{value}"), "id", value);
        }
    }
    for method in ["version.resolve", "install.plan"] {
        for value in [
            serde_json::json!([102, 97, 98]),
            serde_json::json!("版本"),
            serde_json::json!([255]),
            Value::Null,
        ] {
            add(method, &format!("loader-{value}"), "fabric_loader", value);
        }
    }
    let input = temp.path().join("input.json");
    let mut differences = vec![];
    let mut rows = vec![];
    for (method, label, bytes) in &cases {
        fs::write(&input, bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let output = Command::new(&exe).arg(method).arg(&input).output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        let observed = actual(method, bytes);
        let row = serde_json::json!({"method":method,"case":label,"input":String::from_utf8_lossy(bytes),"source":expected,"rust":observed});
        if expected != observed {
            differences.push(row.clone());
        }
        rows.push(row);
        assert_eq!(fs::read(&input).unwrap(), *bytes);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
    }
    if let Ok(path) = std::env::var("JMCL_PARAMS_PROOF_OUTPUT") {
        fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    }
    eprintln!(
        "{} pure helper acceptance/value cases; {} differences; no handler or transport",
        cases.len(),
        differences.len()
    );
    for row in &differences {
        eprintln!("{row}");
    }
    // Keep every original row. These five opaque-byte acceptance gaps are NOT
    // declared compatible; the enum/UTF-8 gate closes the other 101 rows.
    assert_eq!(cases.len(), 106);
    assert_eq!(
        differences.len(),
        5,
        "exact remaining byte-field debt changed"
    );
    let expected_debt: std::collections::BTreeSet<_> = [
        ("version.get", "id-[255]"),
        ("version.resolve", "id-[255]"),
        ("install.plan", "id-[255]"),
        ("version.resolve", "loader-[255]"),
        ("install.plan", "loader-[255]"),
    ]
    .into_iter()
    .collect();
    let observed_debt: std::collections::BTreeSet<_> = differences
        .iter()
        .map(|row| {
            (
                row["method"].as_str().unwrap(),
                row["case"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(observed_debt, expected_debt);
    for row in &differences {
        let label = row["case"].as_str().unwrap();
        assert!(
            label.starts_with("id-[") || label.starts_with("loader-["),
            "unexpected enum/control difference: {row}"
        );
        assert_eq!(row["source"]["accepted"], true);
        assert_eq!(row["rust"]["accepted"], false);
    }
}

#[test]
fn enum_schema_precedes_semantic_guards_without_mutating_input() {
    fn prepare(method: &str, text: &str) -> Result<Value, RpcError> {
        let raw = serde_json::from_str::<Box<RawValue>>(text).unwrap();
        let parsed = parse_params(Some(&raw)).map_err(|_| param_error(method))?;
        let before = parsed.clone();
        let canonical = canonical_params_view(method, parsed.as_ref())?;
        validate_params(method, canonical.as_deref())?;
        let result = canonical.as_deref().unwrap().clone();
        assert_eq!(parsed, before, "caller input must not be changed");
        Ok(result)
    }
    for method in [
        "version.list",
        "version.get",
        "version.resolve",
        "install.plan",
    ] {
        for token in ["-0", "0.0", "0e0", "1e0"] {
            let text = format!(r#"{{"id":"demo","source":{token}}}"#);
            assert_eq!(
                prepare(method, &text),
                Err(param_error(method)),
                "raw float {method}: {text}"
            );
        }
    }
    for (method, text) in [
        ("version.list", r#"{"source":0.0,"limit":0}"#),
        ("version.list", r#"{"type":-0,"limit":0}"#),
        ("version.get", r#"{"id":"","source":"-0"}"#),
        (
            "version.resolve",
            r#"{"id":"","source":2,"fabric_loader":"a","forge_version":"b"}"#,
        ),
        (
            "version.resolve",
            r#"{"id":"demo","source":false,"fabric_loader":"a","forge_version":"b"}"#,
        ),
        ("install.plan", r#"{"id":"","os":"linux","arch":"amd64"}"#),
        ("install.plan", r#"{"id":"","os":3}"#),
        ("version.get", r#"{"id":"demo","source":null}"#),
    ] {
        assert_eq!(
            prepare(method, text),
            Err(param_error(method)),
            "{method}: {text}"
        );
    }
    for method in ["version.get", "version.resolve", "install.plan"] {
        assert_eq!(
            prepare(method, r#"{"id":"","source":1}"#),
            Err(invalid("version id must not be empty"))
        );
    }
    assert_eq!(
        prepare("version.list", r#"{"type":1,"source":0,"limit":0}"#),
        Err(invalid("limit must be between 1 and 1000"))
    );
    assert_eq!(
        prepare(
            "version.resolve",
            r#"{"id":"demo","source":1,"fabric_loader":"a","forge_version":"b"}"#
        ),
        Err(invalid("only one loader field may be set"))
    );
    // Plan's loader conflict is intentionally checked by its later execution
    // body, not moved here by enum preparation; do not execute that body.
    assert!(
        prepare(
            "install.plan",
            r#"{"id":"demo","source":1,"fabric_loader":"a","forge_version":"b"}"#
        )
        .is_ok()
    );
    let input = serde_json::json!({"id":"demo","source":"official","os":null,"arch":null});
    let view = canonical_params_view("install.plan", Some(&input))
        .unwrap()
        .unwrap();
    assert!(matches!(view, std::borrow::Cow::Borrowed(_)));
    assert_eq!(*view, input);
    assert_eq!(
        prepare(
            "install.plan",
            r#"{"id":"demo","source":"+1","os":"0_1","arch":"-00"}"#
        )
        .unwrap(),
        serde_json::json!({"id":"demo","source":"bmclapi","os":"linux","arch":"x86"})
    );
    assert_eq!(
        prepare(
            "version.get",
            r#"{"id":"demo","type":false,"os":{},"arch":[]}"#
        )
        .unwrap(),
        serde_json::json!({"id":"demo","type":false,"os":{},"arch":[]})
    );
    assert!(
        canonical_params_view("version.list", None)
            .unwrap()
            .is_none()
    );
    assert!(validate_params("version.list", None).is_ok());
    assert_eq!(
        actual("version.list", b"{}"),
        serde_json::json!({"accepted":true,"source":"official","type":null,"limit":20})
    );
}

#[test]
fn utf8_byte_parameters_preserve_dynamic_grammar_and_schema_order() {
    fn prepare(method: &str, text: &str) -> Result<Value, RpcError> {
        let raw = serde_json::from_str::<Box<RawValue>>(text).unwrap();
        let parsed = parse_params(Some(&raw)).map_err(|_| param_error(method))?;
        let before = parsed.clone();
        let canonical = canonical_params_view(method, parsed.as_ref())?;
        validate_params(method, canonical.as_deref())?;
        let result = canonical.as_deref().unwrap().clone();
        assert_eq!(parsed, before);
        Ok(result)
    }
    for method in ["version.get", "version.resolve", "install.plan"] {
        for (input, expected) in [
            ("[100,101,109,111]", "demo"),
            ("[231,137,136,230,156,172]", "版本"),
            ("[65.00000000000000001]", "A"),
            (r#"["65","6.6e1","0_67"]"#, "ABC"),
            ("[-0,0.0,0e0]", "\0\0\0"),
        ] {
            let text = format!(r#"{{"id":{input},"source":1}}"#);
            let result = prepare(method, &text).unwrap();
            assert_eq!(result["id"], expected);
            assert_eq!(result["source"], "bmclapi");
        }
        for invalid in [
            r#"["65.00000000000000001"]"#,
            "[65.5]",
            "[-1]",
            "[256]",
            "[true]",
            "[null]",
            "[{}]",
            "[[65]]",
            "null",
            "true",
        ] {
            let text = format!(r#"{{"id":{invalid}}}"#);
            assert_eq!(
                prepare(method, &text),
                Err(param_error(method)),
                "{method}: {text}"
            );
        }
        assert_eq!(
            prepare(method, r#"{"id":[]}"#),
            Err(invalid("version id must not be empty"))
        );
    }
    for method in ["version.resolve", "install.plan"] {
        for key in ["fabric_loader", "neoforge_version", "forge_version"] {
            for (value, expected) in [("[102,97,98]", "fab"), ("[]", ""), ("[231,137,136]", "版")]
            {
                let text = format!(r#"{{"id":[100],"{key}":{value}}}"#);
                assert_eq!(prepare(method, &text).unwrap()[key], expected);
            }
            let text = format!(r#"{{"id":[],"{key}":[256]}}"#);
            assert_eq!(
                prepare(method, &text),
                Err(param_error(method)),
                "schema before empty id"
            );
            let text = format!(r#"{{"id":[100],"{key}":null}}"#);
            assert!(prepare(method, &text).unwrap()[key].is_null());
        }
    }
    assert_eq!(
        prepare(
            "version.resolve",
            r#"{"id":[100],"fabric_loader":[],"forge_version":[]}"#
        ),
        Err(invalid("only one loader field may be set"))
    );
    for (method, text) in [
        ("version.list", r#"{"id":[256],"fabric_loader":false}"#),
        (
            "version.get",
            r#"{"id":[100],"fabric_loader":[256],"future":[false]}"#,
        ),
    ] {
        assert!(
            prepare(method, text).is_ok(),
            "irrelevant fields stay ignored"
        );
    }
    let ordinary = serde_json::json!({"id":"版本","source":"official","forge_version":null});
    let view = canonical_params_view("version.resolve", Some(&ordinary))
        .unwrap()
        .unwrap();
    assert!(matches!(view, std::borrow::Cow::Borrowed(_)));
    assert_eq!(*view, ordinary);
    let unknown = serde_json::json!({"id":[256],"source":false});
    assert_eq!(
        *canonical_params_view("unrelated", Some(&unknown))
            .unwrap()
            .unwrap(),
        unknown
    );
}

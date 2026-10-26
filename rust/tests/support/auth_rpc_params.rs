//! Public dummy buffers only; calls the private decoder, never any auth handler.
use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn mode(n: u8) -> (&'static str, Option<&'static str>) {
    match n {
        0 => ("device_authorization", None),
        1 => ("device_poll", Some("device_code")),
        2 => ("refresh", Some("refresh_token")),
        3 => ("minecraft_exchange", Some("access_token")),
        4 => ("refresh_exchange", Some("refresh_token")),
        _ => unreachable!(),
    }
}
fn bytes_value(v: &[u8]) -> Value {
    match std::str::from_utf8(v) {
        Ok(s) => json!(s),
        Err(_) => json!(v),
    }
}
fn result(raw: &str, n: u8) -> Value {
    let (stage, field) = mode(n);
    let raw = serde_json::value::RawValue::from_string(raw.to_owned()).unwrap();
    parse_rpc_params(&raw, field, stage)
        .map(|(client, secret, locale)| {
            // Only public fixture strings are exposed to this test comparison.
            let mut v = json!({"client_id":bytes_value(&client)});
            if let Some(f) = field {
                v[f] = json!(bytes_value(secret.as_ref().unwrap()));
            }
            if n == 0 {
                v["locale"] = json!(locale.as_ref().map(|v| bytes_value(v)));
            }
            json!({"value":v})
        })
        .unwrap_or_else(|_| json!({"rejected":true}))
}
fn base(n: u8) -> Value {
    let mut v = json!({"client_id":"public-dummy"});
    if let Some(f) = mode(n).1 {
        v[f] = json!("public-dummy");
    }
    v
}
fn token(n: u8, field: &str, t: &str) -> String {
    let mut v = base(n);
    v[field] = json!("TOKEN");
    serde_json::to_string(&v).unwrap().replace("\"TOKEN\"", t)
}
fn agrees(z: &Value, r: &Value) -> bool {
    match (z.get("value"), r.get("value")) {
        (Some(zv), Some(rv)) => {
            z.as_object().is_some_and(|v| v.len() == 1)
                && r.as_object().is_some_and(|v| v.len() == 1)
                && zv == rv
        }
        (None, None) => {
            z.as_object().is_some_and(|v| v.len() == 1)
                && z.get("error")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
                && r == &json!({"rejected":true})
        }
        _ => false,
    }
}
#[test]
fn pure_auth_request_arrays_are_decoded_before_semantics() {
    for n in 0..5 {
        assert_eq!(
            result(&token(n, "client_id", "[80.00000000000000001]"), n)["value"]["client_id"],
            json!("P")
        );
        assert_eq!(
            result(&token(n, "client_id", "[]"), n)["value"]["client_id"],
            json!("")
        );
        assert_eq!(
            result(&token(n, "client_id", "[\"80.00000000000000001\"]"), n),
            json!({"rejected":true})
        );
    }
}
#[test]
fn pure_auth_request_utf8_and_result_shape_controls() {
    // Decoded prefixes must still fail safely; these are error-path controls,
    // not a claim to observe allocator memory after the wiping drop.
    for raw in ["[80,256]", "[80,true]", "[80,1.5]"] {
        assert!(request_text(Json(raw)).is_err());
    }

    assert_eq!(
        result(&token(0, "client_id", "[255]"), 0)["value"]["client_id"],
        json!([255])
    );
    assert_eq!(
        result(&token(0, "client_id", "[195,191]"), 0)["value"]["client_id"],
        json!("ÿ")
    );
    let rejected = json!({"rejected":true});
    assert!(agrees(&json!({"error":"Overflow"}), &rejected));
    for bad in [
        json!({}),
        json!(null),
        json!({"error":false}),
        json!({"error":""}),
        json!({"error":"Overflow","extra":1}),
    ] {
        assert!(!agrees(&bad, &rejected));
    }
    assert!(!agrees(&json!({"error":"Overflow"}), &json!({})));
}
fn build_oracle(exe: &Path) {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required");
    let h = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(h.status.success());
    assert_eq!(
        String::from_utf8(h.stdout).unwrap().trim(),
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
    let src = fs::read_to_string(Path::new(&source).join("src/protocol/auth.zig"))
        .unwrap()
        .replace("\r\n", "\n");
    let fixture = include_str!("../fixtures/auth_rpc_params.zig").replace("\r\n", "\n");
    for name in ["MicrosoftDevicePollParams", "MicrosoftRefreshParams"] {
        let start = src.find(&format!("const {name} = struct {{")).unwrap();
        let end = start + src[start..].find("\n};").unwrap() + 3;
        assert!(fixture.contains(&src[start..end]));
    }
    for name in [
        "MicrosoftDeviceBeginParams",
        "MicrosoftDevicePollParams",
        "MicrosoftRefreshParams",
        "MinecraftExchangeParams",
    ] {
        let mut remaining = src.as_str();
        let needle = format!("std.json.parseFromValue({name},");
        let mut seen = false;
        while let Some(at) = remaining.find(&needle) {
            seen = true;
            remaining = &remaining[at..];
            let a = remaining.find(".{").unwrap() + 2;
            let b = a + remaining[a..].find("})").unwrap();
            assert_eq!(remaining[a..b].trim(), ".ignore_unknown_fields = true,");
            remaining = &remaining[b + 2..];
        }
        assert!(seen);
    }
    // Begin and exchange use the original public schema types directly, without
    // calling their validate methods or any authentication operation.
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let v = Command::new(&zig).arg("version").output().unwrap();
    assert!(v.status.success());
    assert_eq!(String::from_utf8(v.stdout).unwrap().trim(), "0.16.0");
    let o = Command::new(zig)
        .args(["build-exe", "-O", "ReleaseSafe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/auth_rpc_params.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            Path::new(&source).join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
#[test]
#[ignore = "requires pinned pure auth parameter schemas and Zig"]
fn pure_auth_request_decode_matches_pinned_schemas() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    build_oracle(&exe);
    let path = tmp.path().join("input.json");
    let (mut count, mut failures) = (0, vec![]);
    let mut check = |raw: String, n: u8| {
        fs::write(&path, &raw).unwrap();
        let before = fs::metadata(&path).unwrap();
        let o = Command::new(&exe)
            .arg(n.to_string())
            .arg(&path)
            .output()
            .unwrap();
        assert!(o.status.success());
        assert!(o.stderr.is_empty());
        let z: Value = serde_json::from_slice(&o.stdout).unwrap();
        let r = result(&raw, n);
        if !agrees(&z, &r) {
            failures.push(format!("{count}/mode{n}: Rust{r} Zig{z}: {raw}"));
        }
        let after = fs::metadata(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), raw.as_bytes());
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        assert_eq!(
            before.permissions().readonly(),
            after.permissions().readonly()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        }
        count += 1;
    };
    for n in 0..5 {
        let field = mode(n).1.unwrap_or("locale");
        for f in ["client_id", field] {
            for t in [
                "\"public-dummy\"",
                "\"\"",
                "[]",
                "[80]",
                "[80.0]",
                "[80.00000000000000001]",
                "[\"80.00000000000000001\"]",
                "[79.99999999999999999]",
                "[\"79.99999999999999999\"]",
                "[\"+80\"]",
                "[\"8_0\"]",
                "[195,191]",
                "[\"195\",191.0]",
                "[255]",
                "[80,255]",
                "[195,80]",
                "[256]",
                "[80,256]",
                "[-1]",
                "[true]",
                "[{}]",
                "[[80]]",
                "[1e400]",
                "[\"1e400\"]",
                "[18446744073709551616]",
                "null",
                "{}",
                "true",
            ] {
                check(token(n, f, t), n);
            }
        }
        check(serde_json::to_string(&base(n)).unwrap(), n);
        for f in ["client_id", field] {
            let mut v = base(n);
            v.as_object_mut().unwrap().remove(f);
            check(serde_json::to_string(&v).unwrap(), n);
        }
        for unknown in ["1e400", "[{},true,null]", "{\"nested\":false}"] {
            check(
                serde_json::to_string(&base(n)).unwrap().replacen(
                    '{',
                    &format!("{{\"unknown\":{unknown},"),
                    1,
                ),
                n,
            );
        }
        // Non-begin methods ignore locale rather than accidentally validating it.
        for t in ["null", "[]", "{}"] {
            check(token(n, "locale", t), n);
        }
        for reverse in [false, true] {
            let a = "\"client_id\":true";
            let b = format!("\"{field}\":[80.00000000000000001]");
            check(
                format!(
                    "{{{},{}}}",
                    if reverse { b.as_str() } else { a },
                    if reverse { a } else { b.as_str() }
                ),
                n,
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "{count} pure auth decode acceptance/exact value cases matched; rejection names not compared; no handler/validation/transport"
    );
}

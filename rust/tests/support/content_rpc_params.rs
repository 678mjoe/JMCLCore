//! Actual private parsing only: no handler or operation is invoked.
use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn result(raw: &str, dp: bool, change: bool) -> Value {
    let raw = RawValue::from_string(raw.to_owned()).unwrap();
    parse(&raw,dp,change).map(|p|{
  let mut v=json!({"directory":p.directory,"minecraft_version":p.game,"loader":p.loader,"api_key":p.key.as_deref().map(|v|v.as_str()),"project":p.project,"version_id":p.version,"store_directory":p.store,"workers":p.workers,"retries":p.retries});
  if dp{v["world"]=json!(p.world);}
  if !change{v["provider"]=json!(p.provider);v["with_dependencies"]=json!(p.dependencies);}
  json!({"value":v})
 }).unwrap_or_else(|()|json!({"rejected":true}))
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
fn base() -> Value {
    json!({"directory":"owned-placeholder","world":"demo","minecraft_version":"1.20.1","version_id":"V1","project":"m"})
}
fn token(field: &str, raw: &str) -> String {
    let mut v = base();
    v[field] = json!("TOKEN");
    serde_json::to_string(&v).unwrap().replace("\"TOKEN\"", raw)
}
#[test]
fn remote_params_byte_numbers_follow_dynamic_not_typed_rounding() {
    for (dp, change) in [(false, false), (false, true), (true, false), (true, true)] {
        let v = result(&token("project", "[109.00000000000000001]"), dp, change);
        assert_eq!(v["value"]["project"], json!("m"), "{v}");
        assert_eq!(
            result(&token("project", "[\"109.00000000000000001\"]"), dp, change),
            json!({"rejected":true})
        );
    }
}
#[test]
fn remote_params_text_utf8_boundary_is_unchanged() {
    let rejected = json!({"rejected":true});
    assert!(agrees(&json!({"error":"Overflow"}), &rejected));
    for invalid in [
        json!({}),
        json!(null),
        json!({"error":false}),
        json!({"error":""}),
        json!({"error":"Overflow","extra":true}),
        json!({"value":{}}),
    ] {
        assert!(!agrees(&invalid, &rejected));
    }
    assert!(!agrees(&json!({"error":"Overflow"}), &json!({})));

    assert!(text(Json("[255]")).is_err());
    assert_eq!(&**text(Json("[195,191]")).unwrap(), "ÿ");
    assert!(text(Json("[256]")).is_err());
    assert!(text(Json("[true]")).is_err());
}
fn build_oracle(exe: &Path) {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required");
    let head = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
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
    // These are the exact private protocol schema declarations, not a substitute
    // reproduction of handler behavior. Source options are independently pinned.
    let fixture = include_str!("../fixtures/content_rpc_params.zig");
    for (file, names) in [
        (
            "content.zig",
            ["ContentInstallParams", "ContentSetVersionParams"],
        ),
        (
            "datapacks.zig",
            ["DatapackInstallParams", "DatapackSetVersionParams"],
        ),
    ] {
        let src = fs::read_to_string(Path::new(&source).join("src/protocol").join(file))
            .unwrap()
            .replace("\r\n", "\n");
        for name in names {
            let start = src.find(&format!("const {name} = struct {{")).unwrap();
            let end = start + src[start..].find("\n};").unwrap() + 3;
            assert!(
                fixture.replace("\r\n", "\n").contains(&src[start..end]),
                "schema changed: {name}"
            );
            let call = src
                .find(&format!("std.json.parseFromValue({name},"))
                .unwrap();
            let option_start = call + src[call..].find(".{").unwrap() + 2;
            let option_end = option_start + src[option_start..].find("})").unwrap();
            assert_eq!(
                src[option_start..option_end].trim(),
                ".ignore_unknown_fields = true,"
            );
        }
    }
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&zig).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let o = Command::new(zig)
        .args(["build-exe", "-O", "ReleaseSafe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/content_rpc_params.zig",
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
#[ignore = "requires pinned pure protocol Params schema and Zig"]
fn remote_params_buffer_values_match_pinned_dynamic_schemas() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    build_oracle(&exe);
    let path = tmp.path().join("input.json");
    let (mut n, mut failures) = (0, vec![]);
    let mut check = |raw: String, dp: bool, change: bool| {
        fs::write(&path, &raw).unwrap();
        let before = fs::metadata(&path).unwrap();
        let o = Command::new(&exe)
            .arg(format!("{}{}", u8::from(dp), u8::from(change)))
            .arg(&path)
            .output()
            .unwrap();
        assert!(o.status.success());
        assert!(o.stderr.is_empty());
        let z: Value = serde_json::from_slice(&o.stdout).unwrap();
        let r = result(&raw, dp, change);
        let equal = agrees(&z, &r);
        if !equal {
            failures.push(format!("{n}: dp{dp}/change{change} Rust{r} Zig{z}: {raw}"));
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
        n += 1;
    };
    for (dp, change) in [(false, false), (false, true), (true, false), (true, true)] {
        for field in [
            "directory",
            "world",
            "minecraft_version",
            "project",
            "version_id",
            "loader",
            "provider",
            "api_key",
            "store_directory",
        ] {
            for raw in [
                "\"m\"",
                "[109]",
                "[109.0]",
                "[109.00000000000000001]",
                "[\"109.00000000000000001\"]",
                "[108.99999999999999999]",
                "[\"108.99999999999999999\"]",
                "[\"+109\"]",
                "[\"1_0_9\"]",
                "[195,191]",
                "[\"195\",191.0]",
                "[109.5]",
                "[256]",
                "[-1]",
                "[true]",
                "[[109]]",
                "[{}]",
                "[1e400]",
                "[\"1e400\"]",
                "[18446744073709551616]",
                "[\"-0\"]",
                "[-0.0]",
                "[0]",
                "[]",
                "null",
                "true",
            ] {
                check(token(field, raw), dp, change);
            }
        }
        let ordinary = serde_json::to_string(&base()).unwrap();
        check(ordinary.clone(), dp, change);
        for unknown in ["1e400", "[{},true,null]", "{\"nested\":false}"] {
            check(
                ordinary.replacen('{', &format!("{{\"unknown\":{unknown},"), 1),
                dp,
                change,
            );
        }
        for field in [
            "directory",
            "minecraft_version",
            "project",
            "world",
            "version_id",
        ] {
            let mut v = base();
            v.as_object_mut().unwrap().remove(field);
            check(serde_json::to_string(&v).unwrap(), dp, change);
        }
        for early in [true, false] {
            let members = if early {
                "\"project\":[109.00000000000000001],\"workers\":true"
            } else {
                "\"workers\":true,\"project\":[109.00000000000000001]"
            };
            check(
                format!(
                    "{{\"directory\":\"d\",\"world\":\"w\",\"minecraft_version\":\"1\",\"version_id\":\"V\",{members}}}"
                ),
                dp,
                change,
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "{n} pure Params acceptance/exact decoded-value comparisons matched; rejected cases do not claim erased error-name parity; no handler operations"
    );
}

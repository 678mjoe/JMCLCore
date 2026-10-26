//! Pure actual preparation/validators, not an authentication handler or transport.
use super::*;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{fs, path::Path, process::Command};
fn spec(
    n: u64,
) -> (
    &'static str,
    Option<&'static str>,
    &'static str,
    &'static str,
) {
    match n {
        0 => (
            "device_authorization",
            None,
            "auth.microsoft.device.begin requires a Microsoft client ID",
            "Microsoft device authorization parameters are invalid",
        ),
        1 => (
            "device_poll",
            Some("device_code"),
            "auth.microsoft.device.poll requires a client ID and device code",
            "Microsoft device polling parameters are invalid",
        ),
        2 => (
            "refresh",
            Some("refresh_token"),
            "auth.microsoft.refresh requires a client ID and refresh token",
            "Microsoft refresh parameters are invalid",
        ),
        3 => (
            "minecraft_exchange",
            Some("access_token"),
            "auth.minecraft.exchange requires a client ID and Microsoft access token",
            "Minecraft exchange parameters are invalid",
        ),
        4 => (
            "refresh_exchange",
            Some("refresh_token"),
            "auth.minecraft.refresh_exchange requires a client ID and refresh token",
            "Minecraft refresh-exchange parameters are invalid",
        ),
        _ => unreachable!(),
    }
}
fn outcome(raw: Option<&str>, n: u64) -> Value {
    let (stage, field, missing, invalid) = spec(n);
    let raw = raw.map(|s| serde_json::value::RawValue::from_string(s.to_owned()).unwrap());
    match prepare_rpc_params(raw.as_deref(), field, stage, missing, invalid) {
        Err(message) => json!({"code":"INVALID_PARAMS","message":message}),
        Ok((client, secret, locale)) => {
            let semantic = validate_client(client.expose()).and_then(|()| {
                if n == 0 {
                    validate_locale(locale.as_ref().map(Secret::expose))
                } else {
                    validate_opaque(secret.as_ref().unwrap().expose())
                }
            });
            match semantic {
                Ok(()) => json!({"allowed_by_pure_guards":true}),
                Err(e) => {
                    let (code, message) = e.wire();
                    json!({"code":code,"message":message})
                }
            }
        }
    }
}
fn cases() -> Vec<Value> {
    let mut rows: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/auth_preflight_cases.json")).unwrap();
    for n in 0..5 {
        let f = spec(n).1.unwrap_or("locale");
        for (a, b) in [
            ("[80,255]", "[]"),
            ("[]", "[80,255]"),
            ("[255]", "[80,256]"),
            ("[255]", "[80,true]"),
            ("[255]", "[80.5]"),
            ("[255]", "[\"80.00000000000000001\"]"),
            ("[80.00000000000000001]", "[195,191]"),
            ("[\"80.00000000000000001\"]", "[255]"),
            ("[195,191]", "null"),
            ("[80]", "[\"80\"]"),
            ("[195,80]", "\"public-dummy\""),
        ] {
            for reverse in [false, true] {
                let a = format!("\"client_id\":{a}");
                let b = format!("\"{f}\":{b}");
                let raw = if reverse {
                    format!("{{{b},{a}}}")
                } else {
                    format!("{{{a},{b}}}")
                };
                rows.push(json!({"name":"compound-prefix-order","mode":n,"raw":raw}));
            }
        }
    }
    rows
}
#[test]
fn pure_prepare_rejection_precedence() {
    for n in 0..5 {
        let f = spec(n).1.unwrap_or("locale");
        assert_eq!(
            outcome(Some(&format!("{{\"client_id\":[255],\"{f}\":[256]}}")), n),
            json!({"code":"INVALID_PARAMS","message":spec(n).3})
        );
        assert_eq!(
            outcome(
                Some(&format!("{{\"client_id\":[255],\"{f}\":\"public-dummy\"}}")),
                n
            ),
            json!({"code":"INVALID_PARAMS","message":"Authentication parameters are invalid"})
        );
        if n != 0 {
            let msg = match f {
                "device_code" => "Microsoft client ID and device code must not be empty",
                "access_token" => "Microsoft client ID and access token must not be empty",
                _ => "Microsoft client ID and refresh token must not be empty",
            };
            for (a, b) in [("[]", "[255]"), ("[255]", "[]")] {
                assert_eq!(
                    outcome(Some(&format!("{{\"client_id\":{a},\"{f}\":{b}}}")), n),
                    json!({"code":"INVALID_PARAMS","message":msg})
                );
            }
        }
    }
    // Successful preparation preserves exact UTF8 values, not just acceptance.
    let raw = serde_json::value::RawValue::from_string(
        r#"{"client_id":[195,191],"refresh_token":[80.00000000000000001]}"#.into(),
    )
    .unwrap();
    let (s, f, m, i) = spec(2);
    let (c, t, l) = prepare_rpc_params(Some(&raw), f, s, m, i).unwrap();
    assert_eq!(c.expose(), "ÿ");
    assert_eq!(t.unwrap().expose(), "P");
    assert!(l.is_none());
    for locale in ["", ",\"locale\":null"] {
        let raw = serde_json::value::RawValue::from_string(format!(
            "{{\"client_id\":\"public-dummy\"{locale}}}"
        ))
        .unwrap();
        let (s, f, m, i) = spec(0);
        assert!(
            prepare_rpc_params(Some(&raw), f, s, m, i)
                .unwrap()
                .2
                .is_none()
        );
    }
}
fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let target = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}
fn oracle(tmp: &Path) -> std::path::PathBuf {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("explicit pinned source required");
    let source = Path::new(&source);
    let h = Command::new("git")
        .arg("-C")
        .arg(source)
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
            .arg(source)
            .args(["diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    for (path, hash) in [
        (
            "src/auth/authentication.zig",
            "1212aa7a4643876aaa78ff5f6cbc4eaa66938bcd",
        ),
        (
            "src/protocol/auth.zig",
            "3524e7be142a0b0b5cd25d4b0349eae57ee69af2",
        ),
    ] {
        let text = fs::read_to_string(source.join(path))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(
            format!("{:x}", Sha1::digest(text.as_bytes())),
            hash,
            "reviewed pure validator/protocol source changed"
        );
    }
    let copy = tmp.join("src");
    copy_tree(&source.join("src"), &copy);
    let auth = copy.join("auth/authentication.zig");
    let original = fs::read(&auth).unwrap();
    let mut augmented = original.clone();
    augmented.extend_from_slice(include_bytes!("../fixtures/auth_preflight_accessor.zig"));
    fs::write(&auth, augmented).unwrap();
    assert_eq!(
        fs::read(source.join("src/auth/authentication.zig")).unwrap(),
        original
    );
    let exe = tmp.join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let v = Command::new(&zig).arg("version").output().unwrap();
    assert!(v.status.success());
    assert_eq!(String::from_utf8(v.stdout).unwrap().trim(), "0.16.0");
    let out = Command::new(zig)
        .args(["build-exe", "-O", "ReleaseSafe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/auth_preflight.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!("-Mjmcl_core={}", copy.join("root.zig").display()))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}
#[test]
#[ignore = "requires explicit pinned pure auth validators and Zig; never invokes an auth handler"]
fn pure_prepare_matches_pinned_guards() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = oracle(tmp.path());
    let path = tmp.path().join("input.json");
    let rows = cases();
    let mut failures = vec![];
    for row in &rows {
        let raw = row["raw"].as_str();
        let n = row["mode"].as_u64().unwrap();
        fs::write(&path, raw.unwrap_or("{}")).unwrap();
        let before = fs::metadata(&path).unwrap();
        let out = Command::new(&exe)
            .arg(n.to_string())
            .arg(&path)
            .arg(if raw.is_some() { "present" } else { "absent" })
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let z: Value = serde_json::from_slice(&out.stdout).unwrap();
        let r = outcome(raw, n);
        if z != r {
            failures.push(format!("{} mode{n}: Zig{z} Rust{r}", row["name"]));
        }
        let after = fs::metadata(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), raw.unwrap_or("{}").as_bytes());
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
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "{} exact pure preflight outcomes; original pure validators + source-derived protocol guards, not handler execution",
        rows.len()
    );
}

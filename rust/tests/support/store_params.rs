//! Only parameter preparation and in-memory native conversion; never Store/GC.
use super::*;
use sha1::{Digest, Sha1};
use std::{fs, path::Path, process::Command};
fn result(raw: Option<&str>) -> Value {
    let raw = raw.map(|s| RawValue::from_string(s.to_owned()).unwrap());
    match prepare_params(raw.as_deref()) {
        Ok(p) => {
            let path = match std::str::from_utf8(&p.store_directory) {
                Ok(s) => json!(s),
                Err(_) => json!(p.store_directory),
            };
            json!({"value":{"store_directory":path,"dry_run":p.dry_run,"min_age_seconds":p.min_age_seconds}})
        }
        Err(e) => json!({"code":e.code,"message":e.message}),
    }
}
#[test]
fn pure_gc_params_and_native_conversion() {
    let defaults = prepare_params(None).unwrap();
    assert_eq!(defaults.store_directory, DEFAULT_DIRECTORY.as_bytes());
    assert!(defaults.dry_run);
    assert_eq!(defaults.min_age_seconds, 86400);
    assert_eq!(
        prepared_root(&defaults).unwrap(),
        std::ffi::OsString::from(DEFAULT_DIRECTORY)
    );
    for raw in [r#"{"store_directory":[]}"#, r#"{"store_directory":""}"#] {
        assert_eq!(
            result(Some(raw)),
            json!({"code":"INVALID_PARAMS","message":"store.gc store_directory must not be empty"})
        );
    }
    assert_eq!(
        result(Some(r#"{"store_directory":[],"min_age_seconds":true}"#)),
        json!({"code":"INVALID_PARAMS","message":"store.gc parameters are invalid"})
    );
    for bytes in [
        vec![255],
        vec![237, 160, 128],
        vec![0],
        "路径/child".as_bytes().to_vec(),
    ] {
        let raw = RawValue::from_string(json!({"store_directory":bytes}).to_string()).unwrap();
        let p = prepare_params(Some(&raw)).unwrap();
        assert_eq!(p.store_directory, bytes);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(prepared_root(&p).unwrap().as_bytes(), bytes);
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            if bytes == [255] {
                assert_eq!(
                    prepared_root(&p).unwrap_err(),
                    RpcError::new("STORE_ROOT_ERROR", "Failed to open the artifact store")
                );
            } else {
                let expected = if bytes == [237, 160, 128] {
                    vec![0xd800]
                } else if bytes == [0] {
                    vec![0]
                } else {
                    "路径/child".encode_utf16().collect()
                };
                assert_eq!(
                    prepared_root(&p).unwrap().encode_wide().collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
    // Conversion alone says nothing about filesystem acceptance, including NUL.
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
    let protocol = fs::read_to_string(source.join("src/protocol/store.zig"))
        .unwrap()
        .replace("\r\n", "\n");
    let store = fs::read_to_string(source.join("src/install/artifact_store.zig"))
        .unwrap()
        .replace("\r\n", "\n");
    assert!(store.contains("pub const default_directory = \".jmclcore\";"));
    assert_eq!(
        format!("{:x}", Sha1::digest(protocol.as_bytes())),
        "ad10ff7435bdda4ce886697b053d31289c6ca04e"
    );
    let start = protocol.find("const StoreGcParams = struct {").unwrap();
    let end = start + protocol[start..].find("\n};").unwrap() + 3;
    let schema =
        protocol[start..end].replace("core.artifact_store.default_directory", "\".jmclcore\"");
    assert!(
        include_str!("../fixtures/store_params.zig")
            .replace("\r\n", "\n")
            .contains(&schema)
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
    let o = Command::new(zig)
        .args(["build-exe", "-O", "ReleaseSafe"])
        .arg(format!(
            "{}/tests/fixtures/store_params.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    exe
}
#[test]
#[ignore = "requires pinned pure StoreGcParams schema and Zig; never invokes GC"]
fn pure_gc_params_match_pinned_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = oracle(tmp.path());
    let path = tmp.path().join("input.json");
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/store_params_cases.json")).unwrap();
    let mut strict = 0;
    let mut outer = 0;
    let mut failures = vec![];
    for row in rows {
        let raw = row["raw"].as_str();
        fs::write(&path, raw.unwrap_or("{}")).unwrap();
        let before = fs::metadata(&path).unwrap();
        let o = Command::new(&exe)
            .arg(&path)
            .arg(if raw.is_some() { "present" } else { "absent" })
            .output()
            .unwrap();
        assert!(o.status.success());
        assert!(o.stderr.is_empty());
        let z: Value = serde_json::from_slice(&o.stdout).unwrap();
        let r = result(raw);
        if row["name"] == "duplicate-private-seam" {
            assert_eq!(z, json!({"outer_error":"DuplicateField"}));
            assert_eq!(
                r,
                json!({"code":"INVALID_PARAMS","message":"store.gc parameters are invalid"})
            );
            outer += 1;
        } else {
            strict += 1;
            if z != r {
                failures.push(format!("{} Zig{z} Rust{r}: {raw:?}", row["name"]));
            }
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
        "{strict} exact GC parameter outcomes; {outer} separate outer duplicate rejection controls; no Store/open/collect/handler/CLI"
    );
}

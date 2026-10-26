//! Pure typed JSON parsing only. No installation, adoption, archive parsing,
//! provider HTTP, JVM, game/server execution or mutation RPC.
use jmcl_core_rust::content::{curseforge as cf, modrinth as mr, registry};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn registry_fixture() -> Value {
    json!({"schema_version":1,"entries":[{"kind":"mod","file":"mods/a.jar","sha1":"a".repeat(40),"size":1,"source":{"provider":"modrinth","project_id":"Proj1","version_id":"Ver1","slug":"project"},"enabled":true,"as_dependency":false}]})
}
fn project_fixture() -> Value {
    json!({"id":"Proj1","slug":"project","project_type":"mod","title":"A"})
}
fn version_fixture() -> Value {
    json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"url":"https://fixture.invalid/a.jar","filename":"a.jar","hashes":{"sha1":"a".repeat(40)},"size":1}]})
}
fn cf_fixture() -> Value {
    json!({"data":{"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"1".repeat(40)}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]}})
}
fn parse(mode: &str, bytes: &[u8]) -> Value {
    match mode {
        "registry" => registry::parse(bytes)
            .map(|v| serde_json::to_value(v).unwrap())
            .unwrap_or_else(|e| json!({"error":e})),
        "project" => mr::parse_project(bytes)
            .map(|v| serde_json::to_value(v).unwrap())
            .unwrap_or_else(|e| json!({"error":e.0})),
        "version" => mr::parse_version(bytes)
            .map(|v| serde_json::to_value(v).unwrap())
            .unwrap_or_else(|e| json!({"error":e.0})),
        "cffile" => cf::parse_file(bytes)
            .map(|f| json!({"id":f.id,"project":f.project,"name":f.name,"sha1":f.sha1,"size":f.size,"url":f.url}))
            .unwrap_or_else(|e| json!({"error":e.0})),
        _ => panic!("pure parser mode only"),
    }
}
fn array_with_token(mut value: Value, pointer: &str, token: &str) -> Vec<u8> {
    let field = value.pointer_mut(pointer).unwrap();
    let mut bytes: Vec<Value> = field.as_str().unwrap().bytes().map(|b| json!(b)).collect();
    bytes[0] = json!("BYTE_TOKEN");
    *field = Value::Array(bytes);
    serde_json::to_string(&value)
        .unwrap()
        .replace("\"BYTE_TOKEN\"", token)
        .into_bytes()
}
#[test]
fn valid_utf8_registry_arrays_and_typed_provider_bytes() {
    let registry = registry_fixture();
    for pointer in [
        "/entries/0/file",
        "/entries/0/sha1",
        "/entries/0/source/project_id",
        "/entries/0/source/version_id",
        "/entries/0/source/slug",
    ] {
        let byte = registry
            .pointer(pointer)
            .unwrap()
            .as_str()
            .unwrap()
            .as_bytes()[0];
        for token in [byte.to_string(), format!("\"{byte}\""), format!("{byte}.0")] {
            assert_eq!(
                parse(
                    "registry",
                    &array_with_token(registry.clone(), pointer, &token)
                ),
                registry,
                "{pointer}: {token}"
            );
        }
    }
    for (mode, value, pointer) in [
        ("project", project_fixture(), "/title"),
        ("version", version_fixture(), "/files/0/filename"),
        ("cffile", cf_fixture(), "/data/fileName"),
    ] {
        let expected = parse(mode, &serde_json::to_vec(&value).unwrap());
        let byte = value.pointer(pointer).unwrap().as_str().unwrap().as_bytes()[0];
        for token in [format!("\"{byte}\""), format!("{byte}.0")] {
            assert_eq!(
                parse(mode, &array_with_token(value.clone(), pointer, &token)),
                expected,
                "{mode}: {token}"
            );
        }
    }
}
#[test]
fn byte_errors_preserve_typed_width_and_precision() {
    for (token, error) in [
        ("65.0000000000000001", "InvalidNumber"),
        ("\"65.0000000000000001\"", "InvalidNumber"),
        ("256", "Overflow"),
        ("-1", "Overflow"),
    ] {
        assert_eq!(
            parse(
                "project",
                &array_with_token(project_fixture(), "/title", token)
            ),
            json!({"error":error})
        );
        assert_eq!(
            parse(
                "registry",
                &array_with_token(registry_fixture(), "/entries/0/file", token)
            ),
            json!({"error":error})
        );
    }
}
#[test]
fn non_utf8_is_not_silently_reinterpreted() {
    // Explicit unresolved byte-path representation gap, NOT oracle parity.
    let mut p = version_fixture();
    p["files"][0]["filename"] = json!([255]);
    assert_eq!(
        parse("version", &serde_json::to_vec(&p).unwrap()),
        json!({"error":"UnexpectedToken"})
    );
    let mut r = registry_fixture();
    r["entries"][0]["file"] = json!([109, 111, 100, 115, 47, 255, 46, 106, 97, 114]);
    assert_eq!(
        parse("registry", &serde_json::to_vec(&r).unwrap()),
        json!({"error":"UnexpectedToken"})
    );
}
#[test]
fn registry_source_error_names_and_precedence() {
    for (provider, invalid_id) in [
        ("modrinth", "InvalidModrinthVersionId"),
        ("curseforge", "InvalidCurseforgeId"),
    ] {
        let mut value = registry_fixture();
        let source = &mut value["entries"][0]["source"];
        *source =
            json!({"provider":provider,"project_id":"bad/ref","version_id":null,"slug":"bad/ref"});
        assert_eq!(
            parse("registry", &serde_json::to_vec(&value).unwrap()),
            json!({"error":invalid_id})
        );
        value["entries"][0]["source"]["project_id"] = Value::Null;
        assert_eq!(
            parse("registry", &serde_json::to_vec(&value).unwrap()),
            json!({"error":"InvalidContentSource"})
        );
        value["entries"][0]["source"]["project_id"] = json!("123");
        value["entries"][0]["source"]["version_id"] = json!("12");
        assert_eq!(
            parse("registry", &serde_json::to_vec(&value).unwrap()),
            json!({"error":"InvalidModrinthProject"})
        );
    }
}

fn oracle_registry(root: &Path, output: &Path) {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned JMCL_ZIG_SOURCE required");
    let revision = Command::new("git")
        .args(["-C"])
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(revision.status.success());
    assert_eq!(
        String::from_utf8(revision.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    let clean = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["diff", "--quiet", "HEAD", "--", "src"])
        .status()
        .unwrap();
    assert!(clean.success(), "pinned Zig source has local changes");
    let result = Command::new("zig")
        .arg("build-exe")
        .args(["--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}",
            root.join("tests/fixtures/content_registry_strings.zig")
                .display()
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            Path::new(&source).join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", output.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE, zig, and JMCL_CONTENT_METADATA_ORACLE"]
fn typed_string_arrays_match_pinned_pure_parsers() {
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join(if cfg!(windows) {
        "registry.exe"
    } else {
        "registry"
    });
    oracle_registry(Path::new(env!("CARGO_MANIFEST_DIR")), &executable);
    let provider =
        std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").expect("pure provider oracle required");
    let fixture = dir.path().join("input.json");
    let mut count = 0;
    let mut differences = vec![];
    let mut check = |mode: &str, bytes: Vec<u8>| {
        fs::write(&fixture, &bytes).unwrap();
        let before = fs::metadata(&fixture).unwrap();
        let mut cmd = if mode == "registry" {
            Command::new(&executable)
        } else {
            let mut c = Command::new(&provider);
            c.arg(mode);
            c
        };
        let out = cmd.arg(&fixture).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        let zig: Value = serde_json::from_slice(&out.stdout).unwrap();
        let rust = parse(mode, &bytes);
        if rust != zig {
            differences.push(format!(
                "{mode}: {}: Rust {rust}, Zig {zig}",
                String::from_utf8_lossy(&bytes)
            ));
        }
        assert_eq!(fs::read(&fixture).unwrap(), bytes);
        let after = fs::metadata(&fixture).unwrap();
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
    let mut fixtures = vec![
        ("project", project_fixture(), "/title"),
        ("version", version_fixture(), "/files/0/filename"),
        ("cffile", cf_fixture(), "/data/fileName"),
    ];
    for p in [
        "/entries/0/file",
        "/entries/0/sha1",
        "/entries/0/source/project_id",
        "/entries/0/source/version_id",
        "/entries/0/source/slug",
    ] {
        fixtures.push(("registry", registry_fixture(), p));
    }
    for (mode, value, pointer) in fixtures {
        check(mode, serde_json::to_vec(&value).unwrap());
        let byte = value.pointer(pointer).unwrap().as_str().unwrap().as_bytes()[0];
        let tokens = vec![
            byte.to_string(),
            format!("{byte}.0"),
            format!("\"{byte}\""),
            format!("\"+{byte}\""),
            format!("\"{byte}e0\""),
            format!("\"{byte}.0000000000000001\""),
            format!("{byte}.0000000000000001"),
            "-1".into(),
            "256".into(),
            "\"256.0\"".into(),
            "null".into(),
            "true".into(),
            "[]".into(),
            "{}".into(),
            "\"NaN\"".into(),
            "\"0x1.8p+1\"".into(),
        ];
        for token in tokens {
            check(mode, array_with_token(value.clone(), pointer, &token));
        }
    }
    // Plain-string controls distinguish source validation from byte decoding.
    // Both representations must preserve project -> version -> slug precedence.
    for provider in ["modrinth", "curseforge"] {
        let mut base = registry_fixture();
        base["entries"][0]["source"] =
            json!({"provider":provider,"project_id":"123","version_id":"12","slug":"project"});
        let mut cases = vec![];
        for field in ["project_id", "version_id", "slug"] {
            for value in [Value::Null, json!(""), json!("bad/ref")] {
                let mut entry = base.clone();
                entry["entries"][0]["source"][field] = value;
                cases.push(entry);
            }
        }
        for (project, version) in [
            (Value::Null, json!("bad/ref")),
            (json!("bad/ref"), Value::Null),
        ] {
            let mut entry = base.clone();
            entry["entries"][0]["source"]["project_id"] = project;
            entry["entries"][0]["source"]["version_id"] = version;
            cases.push(entry);
        }
        for entry in cases {
            check("registry", serde_json::to_vec(&entry).unwrap());
            let mut array_entry = entry;
            for field in ["project_id", "version_id", "slug"] {
                let value = &mut array_entry["entries"][0]["source"][field];
                if let Some(text) = value.as_str() {
                    *value = json!(text.as_bytes());
                }
            }
            check("registry", serde_json::to_vec(&array_entry).unwrap());
        }
    }
    for field in ["project_id", "version_id", "slug"] {
        for array in [false, true] {
            let mut entry = registry_fixture();
            entry["entries"][0]["source"] = json!({"provider":"manual"});
            entry["entries"][0]["source"][field] = if array { json!([65]) } else { json!("A") };
            check("registry", serde_json::to_vec(&entry).unwrap());
        }
    }
    // UTF-8 multibyte arrays remain strings, not escaped replacement characters.
    for (mode, mut value, pointer) in [
        ("registry", registry_fixture(), "/entries/0/file"),
        ("version", version_fixture(), "/files/0/filename"),
        ("cffile", cf_fixture(), "/data/fileName"),
    ] {
        let text = if mode == "registry" {
            "mods/é.jar"
        } else {
            "é.jar"
        };
        *value.pointer_mut(pointer).unwrap() = json!(text.as_bytes());
        check(mode, serde_json::to_vec(&value).unwrap());
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    eprintln!(
        "{count} exact typed byte-array parser cases; input bytes/mtime/readonly preserved; no runtime operations"
    );
}

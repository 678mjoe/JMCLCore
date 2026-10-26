//! Pure registry parser buffers only; never load/save/mutate a content root.
use jmcl_core_rust::content::registry;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn parse(b: &[u8]) -> Value {
    registry::parse(b)
        .map(|r| serde_json::to_value(r).unwrap())
        .unwrap_or_else(|e| json!({"error":e}))
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn earlier_registry_lexical_error_is_not_displaced() {
    assert_eq!(
        parse(b"{\"unknown\":\"\xff\",\"entries\":true}"),
        json!({"error":"SyntaxError"})
    );
}
fn build_oracle(exe: &Path) {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required");
    let output = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
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
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&zig).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let output = Command::new(zig)
        .args(["build-exe", "-O", "ReleaseSafe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/content_registry_strings.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            Path::new(&source).join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires pinned pure registry source and Zig"]
fn ordered_registry_buffers_match_pinned_source() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    build_oracle(&exe);
    let path = tmp.path().join("input.json");
    let (mut count, mut failures) = (0, vec![]);
    let mut check = |b: Vec<u8>| {
        fs::write(&path, &b).unwrap();
        let before = fs::metadata(&path).unwrap();
        let o = Command::new(&exe).arg(&path).output().unwrap();
        assert!(o.status.success());
        assert!(o.stderr.is_empty());
        let z: Value = serde_json::from_slice(&o.stdout).unwrap();
        let r = parse(&b);
        if z != r {
            failures.push(format!(
                "{count}: Rust{r} Zig{z}: {}",
                String::from_utf8_lossy(&b)
            ));
        }
        let after = fs::metadata(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b);
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
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/registry_ordered_cases.json")).unwrap();
    for r in rows.as_array().unwrap() {
        check(decode(r["hex"].as_str().unwrap()));
    }
    for b in [
        b"{}".as_slice(),
        b"{\"schema_version\":1}",
        b"{\"entries\":[]}",
        b"[]",
        b"",
        b" ",
        b"null",
        b"true",
        b"{\"entries\":true,\"unknown\":\"\xff\"}",
        b"{\"entries\":[],\"entries\":[],\"unknown\":\"\xff\"}",
    ] {
        check(b.to_vec());
    }
    for depth in [1, 160, 512] {
        let mut b = b"{\"unknown\":".to_vec();
        b.extend(vec![b'['; depth]);
        b.extend_from_slice(b"1e400");
        b.extend(vec![b']'; depth]);
        b.extend_from_slice(b"}");
        check(b);
    }

    let entry = json!({"kind":"mod","file":"mods/a.jar","sha1":"a".repeat(40),"size":1,"source":{"provider":"manual"}});
    check(
        format!(
            "{{\"entries\":[{}}}",
            serde_json::to_string(&entry).unwrap()
        )
        .into_bytes(),
    );
    let tokens = [
        "null",
        "true",
        "false",
        "{}",
        "[]",
        "\"\"",
        "0",
        "-0",
        "1",
        "1.0",
        "1.1",
        "1e0",
        "1e400",
        "4294967295",
        "4294967296",
        "18446744073709551615",
        "18446744073709551616",
        "\"+1__0\"",
        "\"0x1p0\"",
        "[109]",
        "[255]",
        "[65.1]",
        "[256]",
        "[{}]",
        "[[]]",
    ];
    for field in [
        "kind",
        "file",
        "sha1",
        "size",
        "source",
        "enabled",
        "as_dependency",
    ] {
        for token in tokens {
            // Public String registry parsing intentionally rejects non-UTF8
            // files. The byte view is tested separately, not normalized here.
            if field == "file" && token == "[255]" {
                continue;
            }
            let mut e = entry.clone();
            e[field] = json!("TOKEN");
            check(
                format!(
                    "{{\"entries\":[{}]}}",
                    serde_json::to_string(&e)
                        .unwrap()
                        .replace("\"TOKEN\"", token)
                )
                .into_bytes(),
            );
        }
    }
    for field in ["provider", "project_id", "version_id", "slug"] {
        for token in tokens {
            let mut e = entry.clone();
            e["source"][field] = json!("TOKEN");
            check(
                format!(
                    "{{\"entries\":[{}]}}",
                    serde_json::to_string(&e)
                        .unwrap()
                        .replace("\"TOKEN\"", token)
                )
                .into_bytes(),
            );
        }
    }
    for token in tokens {
        check(format!("{{\"schema_version\":{token}}}").into_bytes());
    }
    // Every recognized field duplicate, including nullable/defaulted fields, must
    // precede malformed replacement values; repeated unknown fields stay allowed.
    for field in ["provider", "project_id", "version_id", "slug"] {
        let value = if field == "provider" {
            "\"manual\""
        } else {
            "null"
        };
        check(
            format!("{{\"entries\":[{{\"source\":{{\"{field}\":{value},\"{field}\": [}}}}]}}")
                .into_bytes(),
        );
    }
    for field in [
        "kind",
        "file",
        "sha1",
        "size",
        "source",
        "enabled",
        "as_dependency",
    ] {
        let value = serde_json::to_string(entry.get(field).unwrap_or(&json!(true))).unwrap();
        check(format!("{{\"entries\":[{{\"{field}\":{value},\"{field}\":[}}]}}").into_bytes());
    }
    for suffix in ["", ",\"unknown\":1e400", ",\"unknown\":{},\"unknown\":[]"] {
        check(format!("{{\"entries\":[]{suffix}}}").into_bytes());
    }
    // Entire typed array precedes count and semantic validation.
    for n in [1024, 1025] {
        let mut entries = vec![entry.clone(); n];
        check(serde_json::to_vec(&json!({"entries":entries})).unwrap());
        entries[n - 1]["size"] = json!(true);
        check(serde_json::to_vec(&json!({"entries":entries})).unwrap());
    }
    for schema in [0, 1, 2] {
        check(
            serde_json::to_vec(&json!({"schema_version":schema,"entries":[entry.clone()]}))
                .unwrap(),
        );
    }
    for before in [true, false] {
        let malformed = "\"unknown\":\"\\uD800\"";
        let typed = "\"schema_version\":false";
        check(
            format!(
                "{{{},{}}}",
                if before { malformed } else { typed },
                if before { typed } else { malformed }
            )
            .into_bytes(),
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("{count} ordered registry buffers matched");
}

#[test]
fn existing_registry_input_bound_is_preserved() {
    let mut b = b"{}".to_vec();
    b.resize(registry::MAX_BYTES, b' ');
    assert!(registry::parse(&b).is_ok());
    b.push(b' ');
    assert_eq!(registry::parse(&b).unwrap_err(), "ContentRegistryTooLarge");
}

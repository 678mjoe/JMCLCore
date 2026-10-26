//! Pure persisted wire enums only; public serde enums remain named-only.
use jmcl_core_rust::content::registry::{self, Kind, Provider};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn base(kind: &str, provider: &str) -> Value {
    let dir = match kind {
        "mod" => "mods",
        "resourcepack" => "resourcepacks",
        "shaderpack" => "shaderpacks",
        _ => "datapacks",
    };
    let ext = if kind == "mod" { "jar" } else { "zip" };
    let source = match provider {
        "manual" => json!({"provider":provider}),
        "curseforge" => {
            json!({"provider":provider,"project_id":"123","version_id":"456","slug":"project"})
        }
        _ => json!({"provider":provider,"project_id":"Proj1","version_id":"Ver1","slug":"project"}),
    };
    json!({"schema_version":1,"entries":[{"kind":kind,"file":format!("{dir}/a.{ext}"),"sha1":"a".repeat(40),"size":1,"source":source,"enabled":true,"as_dependency":false}]})
}
fn parse(b: &[u8]) -> Value {
    registry::parse(b)
        .map(|r| serde_json::to_value(r).unwrap())
        .unwrap_or_else(|e| json!({"error":e}))
}
fn raw(v: &Value, field: &str, token: &str) -> Vec<u8> {
    let mut v = v.clone();
    let e = &mut v["entries"][0];
    if field == "kind" {
        e[field] = json!("RAW_ENUM")
    } else {
        e["source"][field] = json!("RAW_ENUM")
    };
    serde_json::to_string(&v)
        .unwrap()
        .replace("\"RAW_ENUM\"", token)
        .into_bytes()
}
#[test]
fn persisted_enum_ordinals_are_not_rejected() {
    let b = raw(&base("mod", "manual"), "kind", "0");
    assert!(registry::parse(&b).is_ok());
}
#[test]
fn public_serde_enum_contract_is_unchanged() {
    for token in ["0", "\"0\"", "\"+0\""] {
        assert!(serde_json::from_str::<Kind>(token).is_err());
        assert!(serde_json::from_str::<Provider>(token).is_err());
    }
    assert_eq!(serde_json::from_str::<Kind>("\"mod\"").unwrap(), Kind::Mod);
    assert_eq!(
        serde_json::from_str::<Provider>("\"manual\"").unwrap(),
        Provider::Manual
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
#[ignore = "requires pinned pure registry oracle source and Zig"]
fn persisted_enum_grammar_matches_pinned_source() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    build_oracle(&exe);
    let path = tmp.path().join("input.json");
    let (mut n, mut failures) = (0, vec![]);
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
                "{n}: Rust{r} Zig{z}: {}",
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
        n += 1;
    };
    for (field, names) in [
        (
            "kind",
            vec!["mod", "resourcepack", "shaderpack", "datapack"],
        ),
        ("provider", vec!["modrinth", "curseforge", "manual"]),
    ] {
        for (i, name) in names.iter().enumerate() {
            let v = base(
                if field == "kind" { name } else { "mod" },
                if field == "provider" { name } else { "manual" },
            );
            for token in [
                format!("\"{name}\""),
                i.to_string(),
                format!("\"{i}\""),
                format!("\"+{i}\""),
                format!("\"0_{i}\""),
            ] {
                check(raw(&v, field, &token));
            }
        }
        for token in [
            "-0",
            "-1",
            "0.0",
            "1e0",
            "1e400",
            "3",
            "4",
            "99",
            "18446744073709551616",
            "\"-0\"",
            "\"0.0\"",
            "\"1e0\"",
            "\"0_0\"",
            "\"_0\"",
            "\"0_\"",
            "\"0__0\"",
            "\"+_0\"",
            "\" 0\"",
            "\"0 \"",
            "\"0x0\"",
            "\"wrong\"",
            "\"\"",
            "true",
            "null",
            "[]",
            "{}",
            "{\"unknown\":1}",
        ] {
            check(raw(&base("mod", "modrinth"), field, token));
        }
    }
    for first in [true, false] {
        for enum_token in ["0", "99", "\"wrong\""] {
            let kind = format!("\"kind\":{enum_token}");
            let size = "\"size\":true";
            check(
                format!(
                    "{{\"entries\":[{{{}}}]}}",
                    if first {
                        format!("{kind},{size}")
                    } else {
                        format!("{size},{kind}")
                    }
                )
                .into_bytes(),
            );
        }
    }
    for b in [
        br#"{"entries":[{"kind":0,"kind":99}]}"#.as_slice(),
        br#"{"entries":[{"source":{"provider":0,"provider":99}}]}"#,
        br#"{"entries":[{"kind":99,"size":true}]}"#,
        br#"{"entries":[{"size":true,"kind":99}]}"#,
    ] {
        check(b.to_vec());
    }
    eprintln!("{n} exact persisted enum cases; no load/save/RPC");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

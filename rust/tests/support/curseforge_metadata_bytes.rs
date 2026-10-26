use super::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn file() -> Value {
    json!({"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"a".repeat(40)}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]})
}
fn project() -> Value {
    json!({"id":42,"slug":"project","name":"Name","classId":6})
}
fn parse(mode: &str, doc: &Value, arg: &str, loader: &str) -> Value {
    let bytes = serde_json::to_vec(doc).unwrap();
    let result: mr::Result<Value> = match mode {
        "project" => parse_project(&bytes).map(|p| json!({"name":p.name})),
        "file" => parse_file(&bytes).map(|f| json!({"date":f.date,"games":f.games,"sha1":f.sha1})),
        "supports" => parse_file(&bytes).map(|f| {
            json!(supports(
                &f,
                arg,
                if loader == "none" { None } else { Some(loader) }
            ))
        }),
        "select" => (|| {
            let files: Vec<_> = doc["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| parse_file(&serde_json::to_vec(&json!({"data":f})).unwrap()))
                .collect::<mr::Result<_>>()?;
            Ok(json!(
                select(&files, if arg == "none" { None } else { Some(arg) }).map(|f| f.id)
            ))
        })(),
        _ => panic!("pure metadata only"),
    };
    result.unwrap_or_else(|e| json!({"error":e.0}))
}
#[test]
fn raw_cf_metadata_and_non_sha1_are_retained() {
    let mut p = project();
    p["name"] = json!([255]);
    assert_eq!(
        parse("project", &json!({"data":p}), "", ""),
        json!({"name":[255]})
    );
    let mut f = file();
    f["fileDate"] = json!([255]);
    f["gameVersions"] = json!([[255], "1.20.1", "FaBrIc"]);
    f["hashes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"algo":2,"value":[255]}));
    assert_eq!(
        parse("file", &json!({"data":f}), "", ""),
        json!({"date":[255],"games":[[255],"1.20.1","FaBrIc"],"sha1":"a".repeat(40)})
    );
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and zig; pure parser/selection only"]
fn cf_byte_metadata_selection_matches_pinned_source() {
    let source = std::env::var("JMCL_ZIG_SOURCE").unwrap();
    let head = Command::new("git")
        .args(["-C", &source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(head.status.success());
    assert!(
        Command::new("git")
            .args(["-C", &source, "diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    let build = Command::new("zig")
        .args(["build-exe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/curseforge_metadata_bytes.zig",
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
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let input = temp.path().join("input.json");
    let mut count = 0;
    let mut check = |mode: &str, doc: Value, arg: &str, loader: &str| {
        let bytes = serde_json::to_vec(&doc).unwrap();
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&exe)
            .args([mode, input.to_str().unwrap(), arg, loader])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            parse(mode, &doc, arg, loader),
            expected,
            "{mode} {arg} {loader}: {doc}"
        );
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
        count += 1;
    };
    for value in [
        json!("plain"),
        json!([255]),
        json!([237, 160, 128]),
        json!([195, 169]),
        json!([]),
        json!(["65", 66.0]),
        json!([256]),
        json!([-1]),
        json!([1.5]),
        json!([true]),
        Value::Null,
    ] {
        let mut p = project();
        p["name"] = value.clone();
        check("project", json!({"data":p}), "", "");
        let mut f = file();
        f["fileDate"] = value.clone();
        check("file", json!({"data":f}), "", "");
        let mut f = file();
        f["gameVersions"] = json!([value.clone(), "1.20.1", "Fabric"]);
        check("file", json!({"data":f}), "", "");
        for algo in [1, 2, 255] {
            let mut f = file();
            f["hashes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"algo":algo,"value":value}));
            check("file", json!({"data":f}), "", "");
        }
    }
    for games in [
        json!(["1.20.1", "fAbRiC"]),
        json!([[255], "1.20.1", "Fabric"]),
        json!(["1.20.1", [70, 97, 255, 114, 105, 99]]),
        json!(["1.20.1", "FABRICx"]),
        json!(["1.20.1", "Ｆabric"]),
        json!(["1.20.1", "Forge"]),
        json!(["1.20.1", "NeoForge"]),
        json!(["1.20.10", "Fabric"]),
        json!(["x".repeat(65)]),
        json!([]),
    ] {
        for loader in ["none", "fabric", "forge", "neoforge", "minecraft"] {
            let mut f = file();
            f["gameVersions"] = games.clone();
            check("supports", json!({"data":f}), "1.20.1", loader);
        }
    }
    for (games, game) in [
        (json!(["VersionA", "Fabric"]), "VersionA"),
        (json!(["VersionA", "Fabric"]), "versiona"),
    ] {
        let mut f = file();
        f["gameVersions"] = games;
        check("supports", json!({"data":f}), game, "fabric");
    }
    for hashes in [
        json!([{"algo":2,"value":[255]}]),
        json!([{"algo":1,"value":vec![255;40]}]),
        json!([{"algo":1,"value":"A".repeat(40)},{"algo":1,"value":"b".repeat(40)}]),
        json!([{"algo":1,"value":"a".repeat(40)},{"algo":1,"value":vec![255;40]}]),
    ] {
        let mut f = file();
        f["hashes"] = hashes;
        check("file", json!({"data":f}), "", "");
    }
    let dates = [
        json!("2024"),
        json!("2024-01"),
        json!([255]),
        json!([0]),
        json!([237, 160, 128]),
        json!([195, 169]),
        json!([]),
    ];
    for a in &dates {
        for b in &dates {
            for ext in ["none", ".jar", ".zip"] {
                let mut first = file();
                first["fileDate"] = a.clone();
                let mut second = file();
                second["id"] = json!(2);
                second["fileDate"] = b.clone();
                second["fileName"] = json!("b.zip");
                check("select", json!({"data":[first,second]}), ext, "");
            }
        }
    }
    for available in [false, true] {
        for release in [1, 2, 3] {
            let mut first = file();
            first["fileDate"] = json!([255]);
            first["releaseType"] = json!(release);
            first["isAvailable"] = json!(available);
            let mut second = file();
            second["id"] = json!(2);
            second["fileDate"] = json!([0]);
            check("select", json!({"data":[first,second]}), "none", "");
        }
    }
    eprintln!("{count} exact pure CF byte metadata/supports/selection cases matched");
}

#[test]
fn sha1_integrity_validation_is_not_weakened() {
    let mut f = file();
    f["hashes"] = json!([{"algo":1,"value":vec![255;40]}]);
    assert_eq!(
        parse("file", &json!({"data":f}), "", ""),
        json!({"error":"InvalidCurseforgeHash"})
    );
    let mut f = file();
    f["hashes"] = json!([{"algo":2,"value":[255]}]);
    assert_eq!(
        parse("file", &json!({"data":f}), "", ""),
        json!({"error":"MissingCurseforgeSha1"})
    );
}

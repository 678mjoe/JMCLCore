//! Offline pure schema corpus against rust/compat/modpack_oracle.zig.
use jmcl_core_rust::modpack::schema::{Curseforge, Mrpack, Selection};
use serde_json::{Value, json};
fn loader(s: Option<Selection>) -> Value {
    s.map_or(
        Value::Null,
        |s| json!({"loader":s.loader.name(),"version":s.version}),
    )
}
fn rust(kind: &str, b: &[u8]) -> Value {
    use jmcl_core_rust::modpack::provider;
    match kind {
        "project" => {
            return match provider::project(b) {
                Ok(p) => json!({"class_id":p.class_id,"slug":p.slug}),
                Err(e) => json!({"error":e.name()}),
            };
        }
        "file" => {
            return match provider::file(b) {
                Ok(f) => {
                    json!({"id":f.id,"project":f.project,"name":f.name,"sha1":f.sha1,"size":f.size,"url":f.url})
                }
                Err(e) => json!({"error":e.name()}),
            };
        }
        "url" => {
            return match provider::download_url(b) {
                Ok(v) => json!(v),
                Err(e) => json!({"error":e.name()}),
            };
        }
        _ => {}
    }

    if kind == "mr" {
        match Mrpack::parse(b) {
            Ok(p) => {
                json!({"name":p.name,"version":p.dependencies.minecraft,"loader":loader(p.dependencies.loader().unwrap()),"shipped":p.files.iter().filter(|f|f.ships_to_client()).count()})
            }
            Err(e) => json!({"error":e.name()}),
        }
    } else {
        match Curseforge::parse(b) {
            Ok(p) => {
                json!({"name":p.name,"version":p.minecraft.version,"loader":loader(p.minecraft.loader().unwrap()),"shipped":p.files.iter().filter(|f|f.required).count()})
            }
            Err(e) => json!({"error":e.name()}),
        }
    }
}
#[test]
#[ignore = "requires pinned oracle; mandatory in integrated CI"]
fn schema_oracle() {
    let exe =
        std::env::var_os("JMCL_MODPACK_ORACLE").expect("set required pinned JMCL_MODPACK_ORACLE");
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fixture.json");
    let mut n = 0;
    let mut check = |kind: &str, b: Vec<u8>| {
        std::fs::write(&path, &b).unwrap();
        let out = std::process::Command::new(&exe)
            .arg(kind)
            .arg(&path)
            .output()
            .unwrap();
        assert!(out.status.success());
        let zig: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            rust(kind, &b),
            zig,
            "case {n}: {}",
            String::from_utf8_lossy(&b)
        );
        n += 1;
    };
    let mr = json!({"formatVersion":1,"game":"minecraft","name":"Pack","versionId":"v1","dependencies":{"minecraft":"1.21","fabric-loader":"0.16"},"files":[{"path":"mods/a.jar","hashes":{"sha1":"0123456789012345678901234567890123456789"},"fileSize":5,"downloads":["https://host/a"]}]});
    let cf = json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","minecraft":{"version":"1.21","modLoaders":[]},"files":[{"projectID":1,"fileID":2}]});
    for (kind, base) in [("mr", &mr), ("cf", &cf)] {
        check(kind, serde_json::to_vec(base).unwrap());
        for key in base.as_object().unwrap().keys() {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(key);
            check(kind, serde_json::to_vec(&v).unwrap());
            let mut v = base.clone();
            v[key] = Value::Null;
            check(kind, serde_json::to_vec(&v).unwrap());
        }
        for (key, value) in [("name", json!("")), ("name", json!("x".repeat(257)))] {
            let mut v = base.clone();
            v[key] = value;
            check(kind, serde_json::to_vec(&v).unwrap())
        }
    }
    for (pointer, values) in [
        (
            "/formatVersion",
            vec![
                json!(0),
                json!("1.0"),
                json!(1.0),
                json!(-1),
                json!(1.5),
                json!(4294967296u64),
            ],
        ),
        ("/game", vec![json!("other")]),
        (
            "/dependencies/minecraft",
            vec![json!(""), json!("bad space"), json!("x".repeat(65))],
        ),
        (
            "/files/0/path",
            vec![
                json!("../bad"),
                json!("mods/CON.jar"),
                json!("a//b"),
                json!("a\\b"),
                json!("x".repeat(513)),
            ],
        ),
        (
            "/files/0/fileSize",
            vec![
                json!(0),
                json!(536870913),
                json!("5"),
                json!(5.0),
                json!("5e0"),
            ],
        ),
        (
            "/files/0/downloads",
            vec![
                json!([]),
                json!(["http://x"]),
                json!(["https://"]),
                json!(["https://bad space"]),
            ],
        ),
        (
            "/files/0/hashes/sha1",
            vec![json!("bad"), json!("G".repeat(40))],
        ),
    ] {
        for value in values {
            let mut v = mr.clone();
            *v.pointer_mut(pointer).unwrap() = value;
            check("mr", serde_json::to_vec(&v).unwrap())
        }
    }
    for deps in [
        json!({"minecraft":"1.21","quilt-loader":""}),
        json!({"minecraft":"1.21","fabric-loader":"1","forge":""}),
        json!({"minecraft":"1.21","neoforge":"bad space"}),
    ] {
        let mut v = mr.clone();
        v["dependencies"] = deps;
        check("mr", serde_json::to_vec(&v).unwrap())
    }
    for loaders in [
        json!([{"id":"forge-1","primary":true},{"id":"quilt-1"}]),
        json!([{"id":"fabric-1"},{"id":"forge-1"}]),
        json!([{"id":"forge-"}]),
        json!([{"id":"forge-a b"}]),
        json!([{"id":"forge-1"},{"id":"forge-1"}]),
    ] {
        let mut v = cf.clone();
        v["minecraft"]["modLoaders"] = loaders;
        check("cf", serde_json::to_vec(&v).unwrap())
    }

    let project = json!({"data":{"id":1,"slug":"fixture","name":"Fixture","classId":6}});
    let file = json!({"data":{"id":2,"modId":1,"fileName":"file.jar","releaseType":1,"hashes":[{"algo":1,"value":"0123456789012345678901234567890123456789"}],"fileDate":"2026-01-01","fileLength":5,"gameVersions":["1.21"],"downloadUrl":"https://fixture/file.jar"}});
    for (kind, base) in [("project", project), ("file", file)] {
        check(kind, serde_json::to_vec(&base).unwrap());
        for key in base["data"].as_object().unwrap().keys() {
            let mut v = base.clone();
            v["data"].as_object_mut().unwrap().remove(key);
            check(kind, serde_json::to_vec(&v).unwrap());
            let mut v = base.clone();
            v["data"][key] = Value::Null;
            check(kind, serde_json::to_vec(&v).unwrap());
        }
    }
    for value in [
        Value::Null,
        json!("https://fixture/file.jar"),
        json!("http://bad"),
        json!("https://bad space"),
        json!(""),
    ] {
        check("url", serde_json::to_vec(&json!({"data":value})).unwrap());
    }
    for pointer in ["/dependencies", "/files/0/hashes"] {
        let mut v = mr.clone();
        *v.pointer_mut(pointer).unwrap() = json!([]);
        check("mr", serde_json::to_vec(&v).unwrap());
    }
    check("url", b"{}".to_vec());
    for b in [
        b"".as_slice(),
        b"{",
        b"[",
        b"null",
        b"true",
        b"{} trailing",
        b"{\"name\":\"unterminated",
    ] {
        check("mr", b.to_vec());
    }
    eprintln!("matched {n} schema fixtures");
}

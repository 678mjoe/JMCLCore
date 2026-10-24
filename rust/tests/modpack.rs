use jmcl_core_rust::modpack::{archive, planning::*, schema::*};
use serde_json::{Value, json};
fn mr() -> Value {
    json!({"formatVersion":1,"game":"minecraft","name":"Pack","versionId":"v1","dependencies":{"minecraft":"1.21","fabric-loader":"0.16.1"},"files":[{"path":"mods/a.jar","hashes":{"sha1":"0123456789012345678901234567890123456789"},"fileSize":5,"downloads":["https://cdn.modrinth.com/data/proj/versions/ver/a.jar","https://fallback","https://ignored"]}]})
}
fn cf() -> Value {
    json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","minecraft":{"version":"1.21","modLoaders":[]},"files":[]})
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in entries {
        w.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        w.write_all(content).unwrap()
    }
    w.finish().unwrap().into_inner()
}
#[test]
fn mrpack_download_plan() {
    let mut v = mr();
    let mut skipped = v["files"][0].clone();
    skipped["path"] = json!("mods/server.jar");
    skipped["env"] = json!({"client":"unsupported"});
    v["files"].as_array_mut().unwrap().push(skipped);
    let p = Mrpack::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
    let d = Downloads::mrpack(&p);
    assert_eq!(d.skipped, 1);
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.files[0].fallback_url.as_deref(), Some("https://fallback"));
    assert_eq!(d.entries[0].source.project_id.as_deref(), Some("proj"));
    assert_eq!(d.plan("1.21").unwrap().total_download_size, 5);
}
#[test]
fn validation_precedence() {
    let mut v = mr();
    v["files"][0]["path"] = json!("../bad");
    v["files"][0]["fileSize"] = json!(0);
    assert_eq!(
        Mrpack::parse(&serde_json::to_vec(&v).unwrap())
            .unwrap_err()
            .name(),
        "UnsafePath"
    );
    v["dependencies"]["quilt-loader"] = json!("");
    assert_eq!(
        Mrpack::parse(&serde_json::to_vec(&v).unwrap())
            .unwrap_err()
            .name(),
        "UnsupportedModpackLoader"
    );
}
#[test]
fn duplicate_precedes_later_invalid_file() {
    let mut v = mr();
    let mut other = v["files"][0].clone();
    other["fileSize"] = json!(0);
    v["files"].as_array_mut().unwrap().push(other);
    assert_eq!(
        Mrpack::parse(&serde_json::to_vec(&v).unwrap())
            .unwrap_err()
            .name(),
        "DuplicateModpackFile"
    )
}
#[test]
fn primary_loader_short_circuits() {
    let mut v = cf();
    v["minecraft"]["modLoaders"] =
        json!([{"id":"forge-not a strict version","primary":true},{"id":"quilt-bad"}]);
    let p = Curseforge::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(
        p.minecraft.loader().unwrap().unwrap().version,
        "not a strict version"
    )
}
#[test]
fn optional_curseforge_files_skip_without_provider() {
    struct No;
    impl CurseforgeProvider for No {
        fn project(&mut self, _: u32) -> Result<Project, Error> {
            panic!()
        }
        fn file(&mut self, _: u32, _: u32) -> Result<ProviderFile, Error> {
            panic!()
        }
        fn download_url(&mut self, _: u32, _: u32) -> Result<Option<String>, Error> {
            panic!()
        }
    }
    let mut v = cf();
    v["files"] = json!([{"projectID":1,"fileID":2,"required":false}]);
    let p = Curseforge::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(Downloads::curseforge(&p, &mut No).unwrap().skipped, 1)
}
#[test]
fn archive_format_precedence() {
    let m = serde_json::to_vec(&mr()).unwrap();
    let c = serde_json::to_vec(&cf()).unwrap();
    let b = zip(&[("manifest.json", &c), ("modrinth.index.json", &m)]);
    assert!(matches!(
        Pack::inspect(&b, false).unwrap(),
        Pack::Modrinth(_)
    ));
    let b = zip(&[("manifest.json", b"bad")]);
    assert_eq!(
        Pack::inspect(&b, false).unwrap_err().name(),
        "CurseforgeApiKeyRequired"
    )
}
#[test]
fn override_order_and_private_writer_contract() {
    let b = zip(&[
        ("overrides/config/a.txt", b"one"),
        ("client-overrides/config/a.txt", b"two"),
        ("other", b"ignore"),
    ]);
    let mut writes = vec![];
    for dir in ["overrides", "client-overrides"] {
        assert_eq!(
            overrides(&b, dir, |p, b| {
                writes.push((p.to_string(), b.to_vec()));
                Ok(())
            })
            .unwrap(),
            1
        )
    }
    assert_eq!(writes[1], ("config/a.txt".into(), b"two".to_vec()))
}
#[test]
fn override_traversal_and_directory_records() {
    for path in ["overrides/../bad", "overrides/dir/"] {
        let b = zip(&[(path, b"")]);
        assert_eq!(
            overrides(&b, "overrides", |_, _| panic!())
                .unwrap_err()
                .name(),
            "UnsafePath"
        )
    }
}
#[test]
fn checksum_not_verified_by_legacy_reader() {
    let mut b = zip(&[("a", b"value")]);
    let at = b.windows(5).position(|x| x == b"value").unwrap();
    b[at] = b'V';
    assert_eq!(archive::read_named(&b, "a", 99).unwrap().unwrap(), b"Value")
}
#[test]
fn numeric_compatibility() {
    let mut v = mr();
    v["formatVersion"] = json!("1.0");
    v["files"][0]["fileSize"] = json!(5.0);
    assert!(Mrpack::parse(&serde_json::to_vec(&v).unwrap()).is_ok())
}
#[test]
fn path_guards() {
    for p in [
        "",
        "/x",
        "x\\y",
        "x//y",
        "a/CON.txt",
        "Lpt9",
        "a/b.",
        "a/b ",
        "x:y",
    ] {
        assert!(relative_path(p).is_err(), "{p}")
    }
    assert!(relative_path("资源/文件.jar").is_ok())
}
#[test]
fn known_duplicate_fields_rejected_unknown_ignored() {
    let s = String::from_utf8(serde_json::to_vec(&mr()).unwrap()).unwrap();
    let s = s.replacen('{', "{\"unknown\":1,\"unknown\":2,", 1);
    assert!(Mrpack::parse(s.as_bytes()).is_ok());
    let s = s.replacen('{', "{\"name\":\"duplicate\",", 1);
    assert_eq!(
        Mrpack::parse(s.as_bytes()).unwrap_err().name(),
        "DuplicateField"
    )
}

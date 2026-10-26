use jmcl_core_rust::{
    content::{
        Root, modrinth as mr,
        registry::{Kind, Provider, Registry},
        remote::*,
    },
    install,
    store::Store,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{collections::HashMap, io::Write};
fn version() -> Value {
    json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"url":"https://fixture.invalid/a.jar","filename":"a.jar","hashes":{"sha1":format!("{:x}",Sha1::digest(b"content"))},"size":7}]})
}
#[test]
#[ignore = "requires pinned JMCL_CONTENT_METADATA_ORACLE"]
fn metadata_oracle() {
    let exe = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("fixture.json");
    let mut n = 0;
    let mut check = |kind: &str, b: Vec<u8>| {
        std::fs::write(&path, &b).unwrap();
        let o = std::process::Command::new(&exe)
            .args([kind, path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(o.status.success());
        let zig: Value = serde_json::from_slice(&o.stdout).unwrap();
        let rust = match kind {
            "project" => mr::parse_project(&b).map(|v| serde_json::to_value(v).unwrap()),
            "version" => mr::parse_version(&b).map(|v| serde_json::to_value(v).unwrap()),
            _ => mr::parse_versions(&b).map(|v| serde_json::to_value(v).unwrap()),
        }
        .unwrap_or_else(|e| json!({"error":e.0}));
        assert_eq!(zig, rust, "{kind} {}", String::from_utf8_lossy(&b));
        n += 1;
    };
    let p = json!({"id":"Proj1","slug":"project","project_type":"mod"});
    check("project", serde_json::to_vec(&p).unwrap());
    for field in ["id", "slug", "project_type", "loaders"] {
        for value in [Value::Null, json!("bad/value"), json!(42), json!([])] {
            let mut p = p.clone();
            p[field] = value;
            check("project", serde_json::to_vec(&p).unwrap());
        }
    }
    let v = version();
    check("version", serde_json::to_vec(&v).unwrap());
    let raw = serde_json::to_string(&v)
        .unwrap()
        .replace("\"size\":7", "\"size\":\"7.0000000000000001\"");
    check("version", raw.into_bytes());
    for field in [
        "id",
        "project_id",
        "version_number",
        "version_type",
        "game_versions",
        "loaders",
        "files",
        "dependencies",
    ] {
        for value in [Value::Null, json!("bad/value"), json!(42), json!([])] {
            let mut v = v.clone();
            v[field] = value;
            check("version", serde_json::to_vec(&v).unwrap());
        }
    }
    for field in ["filename", "url", "size", "primary", "hashes"] {
        for value in [
            Value::Null,
            json!("bad/value"),
            json!(0),
            json!("1_0"),
            json!([]),
        ] {
            let mut v = v.clone();
            v["files"][0][field] = value;
            check("version", serde_json::to_vec(&v).unwrap());
        }
    }
    for name in ["CON.jar", "aux.zip", "normal.jar", "a.", "../a", "é.jar"] {
        let mut v = v.clone();
        v["files"][0]["filename"] = json!(name);
        check("version", serde_json::to_vec(&v).unwrap());
    }
    check("versions", serde_json::to_vec(&json!([v])).unwrap());
    check("versions", b"[]".to_vec());
    eprintln!("{n} content metadata oracle fixtures passed");
}
struct Fixture {
    docs: HashMap<String, Vec<u8>>,
    fail: bool,
}
impl Metadata for Fixture {
    fn get(&self, u: &str, limit: usize) -> mr::Result<Vec<u8>> {
        let b = self
            .docs
            .get(u)
            .ok_or_else(|| mr::Error("MissingFixture".into()))?;
        assert!(b.len() <= limit);
        Ok(b.clone())
    }
}
impl install::Transport for Fixture {
    fn fetch(&self, _: &str, w: &mut dyn Write) -> Result<u16, install::Error> {
        w.write_all(if self.fail { b"bad" } else { b"content" })?;
        Ok(200)
    }
}
fn fixture() -> Fixture {
    let mut f = Fixture {
        docs: HashMap::new(),
        fail: false,
    };
    f.docs.insert(
        mr::project_url("project").unwrap(),
        serde_json::to_vec(&json!({"id":"Proj1","slug":"project","project_type":"mod"})).unwrap(),
    );
    f.docs.insert(
        mr::versions_url("Proj1", "1.20.1", Some("fabric")).unwrap(),
        serde_json::to_vec(&json!([version()])).unwrap(),
    );
    f
}
#[test]
fn resolves_installs_and_preserves_registry_on_failure() {
    let t = tempfile::tempdir().unwrap();
    let game = t.path().join("game");
    std::fs::create_dir(&game).unwrap();
    let root = Root::open(game.to_str().unwrap()).unwrap();
    let store = Store::open(t.path().join("cas")).unwrap();
    let mut f = fixture();
    let r = resolve_modrinth(
        &f,
        Kind::Mod,
        "1.20.1",
        Some("fabric"),
        "project",
        None,
        true,
        &Registry::default(),
    )
    .unwrap();
    assert_eq!(r.files.len(), 1);
    f.fail = true;
    assert!(
        root.install_resolved(
            &store,
            Kind::Mod,
            "1.20.1",
            Provider::Modrinth,
            r,
            install::Options::default(),
            &f,
            None
        )
        .is_err()
    );
    assert!(!game.join(".jmcl/content.json").exists());
    f.fail = false;
    let r = resolve_modrinth(
        &f,
        Kind::Mod,
        "1.20.1",
        Some("fabric"),
        "project",
        None,
        true,
        &root.load().unwrap(),
    )
    .unwrap();
    let s = root
        .install_resolved(
            &store,
            Kind::Mod,
            "1.20.1",
            Provider::Modrinth,
            r,
            install::Options::default(),
            &f,
            None,
        )
        .unwrap();
    assert_eq!(s.installed.len(), 1);
    assert_eq!(std::fs::read(game.join("mods/a.jar")).unwrap(), b"content");
    let r = resolve_modrinth(
        &f,
        Kind::Mod,
        "1.20.1",
        Some("fabric"),
        "project",
        None,
        true,
        &root.load().unwrap(),
    )
    .unwrap();
    assert_eq!(r.skipped, 1);
    assert!(r.files.is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let obj = t
            .path()
            .join("cas")
            .join(jmcl_core_rust::store::object_path(&s.installed[0].sha1).unwrap());
        assert_eq!(
            std::fs::metadata(obj).unwrap().ino(),
            std::fs::metadata(game.join("mods/a.jar")).unwrap().ino()
        );
    }
}
#[test]
fn stable_selection_and_dependency_rules() {
    let mut beta: mr::Version =
        mr::parse_version(&serde_json::to_vec(&version()).unwrap()).unwrap();
    beta.version_type = "beta".into();
    let mut release = beta.clone();
    release.version_type = "release".into();
    release.id = "Release".into();
    let versions = vec![beta, release];
    assert_eq!(
        mr::select(&versions, "1.20.1", Some("fabric")).unwrap().id,
        "Release"
    );
    assert!(mr::select(&versions, "other", None).is_none());
    assert_eq!(
        resolve_modrinth(
            &fixture(),
            Kind::Mod,
            "1.20.1",
            None,
            "project",
            None,
            true,
            &Registry::default()
        )
        .err()
        .unwrap()
        .0,
        "ModLoaderRequired"
    );
}
#[test]
fn dependency_resolution_order_cycle_and_version_switch() {
    let mut f = fixture();
    let mut primary = version();
    primary["dependencies"] = json!([{"project_id":"Dep1","version_id":"Ignored","dependency_type":"required"},{"project_id":"Skip1","dependency_type":"optional"}]);
    f.docs.insert(
        mr::versions_url("Proj1", "1.20.1", Some("fabric")).unwrap(),
        serde_json::to_vec(&json!([primary])).unwrap(),
    );
    f.docs.insert(
        mr::project_url("Dep1").unwrap(),
        serde_json::to_vec(&json!({"id":"Dep1","slug":"dependency","project_type":"mod"})).unwrap(),
    );
    let mut dep = version();
    dep["id"] = json!("DepVer");
    dep["project_id"] = json!("Dep1");
    dep["files"][0]["filename"] = json!("dependency.jar");
    dep["dependencies"] = json!([{"project_id":"project","dependency_type":"required"}]);
    f.docs.insert(
        mr::versions_url("Dep1", "1.20.1", Some("fabric")).unwrap(),
        serde_json::to_vec(&json!([dep])).unwrap(),
    );
    let r = resolve_modrinth(
        &f,
        Kind::Mod,
        "1.20.1",
        Some("fabric"),
        "project",
        None,
        true,
        &Registry::default(),
    )
    .unwrap();
    assert_eq!(
        r.files
            .iter()
            .map(|f| f.project_id.as_str())
            .collect::<Vec<_>>(),
        ["Proj1", "Dep1"]
    );
    assert!(!r.files[0].as_dependency);
    assert!(r.files[1].as_dependency);
    let t = tempfile::tempdir().unwrap();
    let game = t.path().join("game");
    std::fs::create_dir(&game).unwrap();
    let root = Root::open(game.to_str().unwrap()).unwrap();
    let store = Store::open(t.path().join("cas")).unwrap();
    let events = std::sync::Mutex::new(Vec::new());
    let report = |event| {
        events.lock().unwrap().push(format!("{event:?}"));
        Ok(())
    };
    let outcome = root.install_resolved(
        &store,
        Kind::Mod,
        "1.20.1",
        Provider::Modrinth,
        r,
        install::Options::default(),
        &f,
        Some(&report),
    );
    if let Err(error) = outcome {
        // Synthetic test paths only; never enabled in production. Preserve the
        // failure and tree before cleanup, without retrying or repairing it.
        fn tree(path: &std::path::Path, prefix: &str) {
            let mut entries = match std::fs::read_dir(path) {
                Ok(entries) => entries.collect::<Result<Vec<_>, _>>().unwrap(),
                Err(e) => {
                    eprintln!("{prefix}: read_dir {e:?}");
                    return;
                }
            };
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
                let metadata = entry.path().symlink_metadata().unwrap();
                eprintln!("{name}: {metadata:?}");
                if metadata.is_dir() {
                    tree(&entry.path(), &name);
                }
            }
        }
        eprintln!("install events: {:#?}", events.lock().unwrap());
        tree(t.path(), "fixture");
        panic!("two same-digest content files failed: {error:?}");
    }

    root.toggle(Kind::Mod, Some("project"), None, false)
        .unwrap();
    let mut next = version();
    next["id"] = json!("Ver2");
    next["files"][0]["filename"] = json!("new.jar");
    f.docs.insert(
        mr::version_url("Ver2").unwrap(),
        serde_json::to_vec(&next).unwrap(),
    );
    let c = root
        .set_version(
            &store,
            Kind::Mod,
            "1.20.1",
            Some("fabric"),
            "project",
            "Ver2",
            None,
            &f,
            &f,
            install::Options::default(),
            None,
        )
        .unwrap();
    assert!(c.changed);
    assert!(!game.join("mods/a.jar.disabled").exists());
    assert!(game.join("mods/new.jar.disabled").exists());
    assert!(!game.join("mods/new.jar").exists());
    let reg = root.load().unwrap();
    assert!(!reg.entries[0].enabled);
    assert_eq!(reg.entries[0].source.version_id.as_deref(), Some("Ver2"));
    let c = root
        .set_version(
            &store,
            Kind::Mod,
            "1.20.1",
            Some("fabric"),
            "project",
            "Ver2",
            None,
            &Fixture {
                docs: HashMap::new(),
                fail: true,
            },
            &f,
            install::Options::default(),
            None,
        )
        .unwrap();
    assert!(!c.changed);
}
#[test]
fn curseforge_selection_and_borrowed_key_boundary() {
    use jmcl_core_rust::content::curseforge as cf;
    struct Cf {
        docs: HashMap<String, Vec<u8>>,
    }
    impl cf::Metadata for Cf {
        fn get(&self, url: &str, key: &str, _: usize) -> mr::Result<Vec<u8>> {
            assert_eq!(key, "synthetic-key");
            assert!(url.starts_with("https://api.curseforge.com/"));
            self.docs
                .get(url)
                .cloned()
                .ok_or_else(|| mr::Error("MissingFixture".into()))
        }
    }
    let file = |id: u32, release: u8, date: &str| json!({"id":id,"modId":42,"fileName":format!("{id}.jar"),"releaseType":release,"hashes":[{"value":format!("{:x}",Sha1::digest(b"content")),"algo":1}],"fileDate":date,"fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]});
    let mut docs = HashMap::new();
    docs.insert(
        "https://api.curseforge.com/v1/mods/42".into(),
        serde_json::to_vec(
            &json!({"data":{"id":42,"slug":"project","name":"Project","classId":6}}),
        )
        .unwrap(),
    );
    // Page bound is 8 MiB; individual file envelopes use 2 MiB, but an
    // ignored field inside a page must not accidentally inherit that tighter cap.
    let mut wide = file(1, 2, "2025");
    wide["ignored"] = json!("x".repeat(2 * 1024 * 1024 + 1));
    docs.insert(
        cf::page_url("42", "1.20.1", Some("fabric"), 0).unwrap(),
        serde_json::to_vec(&json!({"data":[wide,file(2,1,"2023"),file(3,1,"2024")]})).unwrap(),
    );
    docs.insert(
        "https://api.curseforge.com/v1/mods/42/files/3/download-url".into(),
        serde_json::to_vec(&json!({"data":"https://cdn.invalid/3.jar"})).unwrap(),
    );
    let r = cf::resolve(
        &Cf { docs },
        Kind::Mod,
        "1.20.1",
        Some("fabric"),
        "synthetic-key",
        "42",
        None,
        true,
        &Registry::default(),
    )
    .unwrap();
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.files[0].version_id, "3");
    assert_eq!(r.files[0].url, "https://cdn.invalid/3.jar");
    assert!(!format!("{:?}", r.files).contains("synthetic-key"));
}

impl jmcl_core_rust::content::curseforge::Metadata for Fixture {
    fn get(&self, _: &str, _: &str, _: usize) -> mr::Result<Vec<u8>> {
        Err("MissingFixture".into())
    }
}
#[test]
#[ignore = "requires pinned JMCL_CONTENT_METADATA_ORACLE"]
fn curseforge_schema_oracle() {
    let exe = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("cf.json");
    let base = json!({"data":{"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"1111111111111111111111111111111111111111"}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]}});
    let mut count = 0;
    let mut check = |v: Value| {
        let b = serde_json::to_vec(&v).unwrap();
        std::fs::write(&path, &b).unwrap();
        let o = std::process::Command::new(&exe)
            .args(["cffile", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(o.status.success());
        let z: Value = serde_json::from_slice(&o.stdout).unwrap();
        let r = match jmcl_core_rust::content::curseforge::parse_file(&b) {
            Ok(f) => {
                json!({"id":f.id,"project":f.project,"name":f.name,"sha1":f.sha1,"size":f.size,"url":f.url})
            }
            Err(e) => json!({"error":e.0}),
        };
        assert_eq!(z, r, "{v}");
        count += 1;
    };
    check(base.clone());
    for field in [
        "id",
        "modId",
        "fileName",
        "releaseType",
        "hashes",
        "fileDate",
        "fileLength",
        "downloadUrl",
        "gameVersions",
        "dependencies",
    ] {
        for value in [
            json!(null),
            json!([]),
            json!("1_0"),
            json!(-1),
            json!(256),
            json!(1.5),
            json!("-1"),
            json!("-0"),
            json!("4294967296"),
            json!("18446744073709551616"),
            json!(true),
        ] {
            let mut v = base.clone();
            v["data"][field] = value;
            check(v)
        }
    }
    for value in [
        json!({}),
        json!([]),
        json!([[]]),
        json!([{"algo":1,"value":[]}]),
    ] {
        let mut v = base.clone();
        v["data"]["hashes"] = value;
        check(v)
    }
    eprintln!("{count} CurseForge schema fixtures matched");
}

#[test]
#[ignore = "requires pinned JMCL_CONTENT_METADATA_ORACLE; pure JSON only"]
fn provider_numeric_lexemes_match_pinned_typed_grammar() {
    use jmcl_core_rust::content::curseforge as cf;
    let exe = std::env::var_os("JMCL_CONTENT_METADATA_ORACLE").unwrap();
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("numeric.json");
    let tokens = [
        "7",
        "7.0",
        "7e0",
        "7.0000000000000001",
        "\"7.0000000000000001\"",
        "\"7.0\"",
        "\"+7\"",
        "\"7__0\"",
        "\"0x7.0p0\"",
        "\"7e+\"",
        "\"7_.0\"",
        "-0",
        "\"-0\"",
        "-1",
        "\"-1\"",
        "255",
        "256",
        "255.0000000000000001",
        "4294967295",
        "4294967296",
        "4294967295.0000000001",
        "18446744073709551615",
        "\"18446744073709551615.0\"",
        "18446744073709551616",
        "1e400",
        "null",
        "true",
        "[]",
        "{}",
        "\"\"",
        "\"+_7\"",
        "\"7_\"",
        "\"7\\u002e0000000000000001\"",
        "7.00000000000000000000000000000000001",
    ];
    let cfbase = json!({"data":{"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"1111111111111111111111111111111111111111"}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]}});
    let mut count = 0;
    for (kind, field) in [
        ("version", "size"),
        ("cffile", "fileLength"),
        ("cffile", "id"),
        ("cffile", "modId"),
        ("cffile", "releaseType"),
        ("cffile", "algo"),
    ] {
        for token in tokens {
            let mut v = if kind == "version" {
                version()
            } else {
                cfbase.clone()
            };
            if kind == "version" {
                v["files"][0][field] = json!("NUMBER_PLACEHOLDER");
            } else if field == "algo" {
                v["data"]["hashes"][0][field] = json!("NUMBER_PLACEHOLDER");
            } else {
                v["data"][field] = json!("NUMBER_PLACEHOLDER");
            }
            let bytes = serde_json::to_string(&v)
                .unwrap()
                .replace("\"NUMBER_PLACEHOLDER\"", token)
                .into_bytes();
            std::fs::write(&path, &bytes).unwrap();
            let out = std::process::Command::new(&exe)
                .args([kind, path.to_str().unwrap()])
                .output()
                .unwrap();
            assert!(out.status.success());
            let old: Value = serde_json::from_slice(&out.stdout).unwrap();
            let new=if kind=="version" {mr::parse_version(&bytes).map(|v|serde_json::to_value(v).unwrap())}
            else {cf::parse_file(&bytes).map(|f|json!({"id":f.id,"project":f.project,"name":f.name,"sha1":f.sha1,"size":f.size,"url":f.url}))}.unwrap_or_else(|e|json!({"error":e.0}));
            assert_eq!(new, old, "{kind}.{field} raw token {token}");
            count += 1;
        }
    }
    eprintln!("{count} raw provider typed-number cases matched; no network or artifact access");
}

#[test]
fn invalid_typed_metadata_prevents_download_and_preserves_registry() {
    fn identity(path: &std::path::Path) -> (u64, u64) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let m = std::fs::metadata(path).unwrap();
            (m.dev(), m.ino())
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
            };
            let f = std::fs::File::open(path).unwrap();
            let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
            assert_ne!(
                unsafe { GetFileInformationByHandle(f.as_raw_handle(), info.as_mut_ptr()) },
                0
            );
            let info = unsafe { info.assume_init() };
            (
                u64::from(info.dwVolumeSerialNumber),
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            )
        }
    }

    use jmcl_core_rust::content::registry::{Entry, Source};
    struct NoDownload;
    impl install::Transport for NoDownload {
        fn fetch(&self, _: &str, _: &mut dyn Write) -> Result<u16, install::Error> {
            panic!("invalid metadata must never reach artifact transport")
        }
    }
    let t = tempfile::tempdir().unwrap();
    let game = t.path().join("game");
    std::fs::create_dir_all(game.join("mods")).unwrap();
    let root = Root::open(game.to_str().unwrap()).unwrap();
    let old = game.join("mods/a.jar");
    std::fs::write(&old, b"content").unwrap();
    std::fs::hard_link(&old, t.path().join("sibling.jar")).unwrap();
    let mut perms = std::fs::metadata(&old).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&old, perms).unwrap();
    root.save(&Registry {
        entries: vec![Entry {
            kind: Kind::Mod,
            file: "mods/a.jar".into(),
            sha1: format!("{:x}", Sha1::digest(b"content")),
            size: 7,
            source: Source {
                provider: Provider::Modrinth,
                project_id: Some("Proj1".into()),
                version_id: Some("Ver1".into()),
                slug: Some("project".into()),
            },
            enabled: true,
            as_dependency: false,
        }],
        ..Registry::default()
    })
    .unwrap();
    let registry = std::fs::read(game.join(".jmcl/content.json")).unwrap();
    let store = Store::open(t.path().join("cas")).unwrap();
    let old_identity = identity(&old);
    let registry_path = game.join(".jmcl/content.json");
    let registry_identity = identity(&registry_path);
    let mut f = fixture();
    let mut next = version();
    next["id"] = json!("Ver2");
    next["files"][0]["filename"] = json!("new.jar");
    for token in ["7.0000000000000001", "\"7.0000000000000001\""] {
        let raw = serde_json::to_string(&next)
            .unwrap()
            .replace("\"size\":7", &format!("\"size\":{token}"));
        f.docs
            .insert(mr::version_url("Ver2").unwrap(), raw.into_bytes());
        let error = root
            .set_version(
                &store,
                Kind::Mod,
                "1.20.1",
                Some("fabric"),
                "project",
                "Ver2",
                None,
                &f,
                &NoDownload,
                install::Options::default(),
                None,
            )
            .expect_err("invalid typed size must fail");
        assert!(format!("{error:?}").contains("InvalidNumber"), "{error:?}");
        assert_eq!(
            std::fs::read(game.join(".jmcl/content.json")).unwrap(),
            registry
        );
        for p in [&old, &t.path().join("sibling.jar")] {
            assert_eq!(std::fs::read(p).unwrap(), b"content");
            assert!(std::fs::metadata(p).unwrap().permissions().readonly());
            assert_eq!(identity(p), old_identity);
        }
        assert_eq!(identity(&registry_path), registry_identity);
        assert!(!game.join("mods/new.jar").exists());
        assert_eq!(std::fs::read_dir(game.join("mods")).unwrap().count(), 1);
    }
}

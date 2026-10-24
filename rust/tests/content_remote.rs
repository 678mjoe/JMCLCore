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
    root.install_resolved(
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
        .set_modrinth_version(
            &store,
            Kind::Mod,
            "1.20.1",
            Some("fabric"),
            "project",
            "Ver2",
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
        .set_modrinth_version(
            &store,
            Kind::Mod,
            "1.20.1",
            Some("fabric"),
            "project",
            "Ver2",
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

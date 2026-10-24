//! Opt-in pinned Zig differential coverage; fixtures are disposable and credential-free.
use jmcl_core_rust::content::{
    self, Root,
    registry::{Entry, Kind, Provider, Registry, Source},
};
use serde_json::{Value, json, value::RawValue};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
fn zig(method: &str, params: Option<Value>) -> Vec<u8> {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").expect("JMCL_ZIG_ORACLE required");
    let mut req = json!({"protocol":1,"id":"c","method":method});
    if let Some(p) = params {
        req["params"] = p;
    }
    let mut p = Command::new(oracle)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(p.stdin.take().unwrap(), "{req}").unwrap();
    let o = p.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    o.stdout
}
fn rust(method: &str, params: Option<Value>) -> Vec<u8> {
    let raw = params.map(|p| RawValue::from_string(p.to_string()).unwrap());
    let response = match content::handle(method, raw.as_deref()).unwrap() {
        Ok(v) => json!({"protocol":1,"id":"c","event":"result","result":v}),
        Err(e) => {
            json!({"protocol":1,"id":"c","event":"error","error":{"code":e.code,"message":e.message}})
        }
    };
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    bytes
}
fn same(method: &str, p: Option<Value>) {
    assert_eq!(
        String::from_utf8(rust(method, p.clone())).unwrap(),
        String::from_utf8(zig(method, p.clone())).unwrap(),
        "{method}: {p:?}"
    )
}
fn entry(kind: Kind, name: &str, provider: Provider, id: Option<&str>) -> Entry {
    Entry {
        kind,
        file: format!("{}/{name}", kind.directory()),
        sha1: "a".repeat(40),
        size: 1,
        source: Source {
            provider,
            project_id: id.map(str::to_string),
            version_id: id.map(|_| "12".into()),
            slug: None,
        },
        enabled: true,
        as_dependency: false,
    }
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn content_params_and_empty_roots_match() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().to_str().unwrap();
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for op in ["list", "enable", "disable", "remove"] {
            let method = format!("{family}.{op}");
            for p in [
                None,
                Some(json!({})),
                Some(json!([])),
                Some(json!({"directory":""})),
                Some(json!({"directory":path,"world":"Missing"})),
                Some(json!({"directory":path,"world":"Missing","file":"mods/a.jar","project":"a"})),
            ] {
                same(&method, p)
            }
        }
    }
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn shared_registry_and_provider_selectors_match() {
    let t = tempfile::tempdir().unwrap();
    let root = Root::open(t.path().to_str().unwrap()).unwrap();
    fs::create_dir(t.path().join("mods")).unwrap();
    fs::write(t.path().join("mods/unmanaged.jar"), b"x").unwrap();
    root.save(&Registry {
        entries: vec![
            entry(Kind::Mod, "curse.jar", Provider::Curseforge, Some("123")),
            entry(Kind::Mod, "modrinth.jar", Provider::Modrinth, Some("123")),
            entry(Kind::Shaderpack, "shader.zip", Provider::Manual, None),
        ],
        ..Registry::default()
    })
    .unwrap();
    for family in ["mods", "resourcepacks", "shaderpacks"] {
        same(
            &format!("{family}.list"),
            Some(json!({"directory":t.path()})),
        );
    }
    for selector in [
        json!({"project":"missing"}),
        json!({"file":"../escape"}),
        json!({"project":"bad/ref"}),
        json!({"file":"mods/missing.jar"}),
    ] {
        let mut p = selector;
        p["directory"] = json!(t.path());
        same("mods.remove", Some(p.clone()));
        same("mods.disable", Some(p));
    }
    // Zig consumes Rust-authored registry; Rust consumes Zig's rewritten registry.
    let off = zig(
        "mods.disable",
        Some(json!({"directory":t.path(),"project":"123"})),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&off).unwrap()["result"]["entry"]["source"]["provider"],
        "curseforge"
    );
    let registry = root.load().unwrap();
    assert!(!registry.entries[0].enabled);
    assert!(registry.entries[1].enabled);
    let removed = zig(
        "mods.remove",
        Some(json!({"directory":t.path(),"project":"123"})),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&removed).unwrap()["result"]["removed"]["source"]["provider"],
        "modrinth"
    );
    assert_eq!(root.load().unwrap().entries.len(), 2);
    same("mods.list", Some(json!({"directory":t.path()})));
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn world_datapack_lock_and_invalid_names_match() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("saves/World")).unwrap();
    let p = json!({"directory":t.path(),"world":"World"});
    same("datapacks.list", Some(p));
    let lock = fs::File::create(t.path().join("saves/World/session.lock")).unwrap();
    lock.lock().unwrap();
    for op in ["remove", "enable", "disable"] {
        same(
            &format!("datapacks.{op}"),
            Some(json!({"directory":t.path(),"world":"World","file":"datapacks/a.zip"})),
        );
    }
    lock.unlock().unwrap();
    same(
        "datapacks.remove",
        Some(json!({"directory":t.path(),"world":"World","file":"datapacks/a.zip"})),
    );
    same(
        "datapacks.list",
        Some(json!({"directory":t.path(),"world":"../bad"})),
    );
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn mutations_and_disk_rewrite_match() {
    for (kind, name) in [
        (Kind::Mod, "a.jar"),
        (Kind::Resourcepack, "a.zip"),
        (Kind::Shaderpack, "a.zip"),
        (Kind::Datapack, "a.zip"),
    ] {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for t in [&a, &b] {
            let path = if kind == Kind::Datapack {
                t.path().join("saves/World")
            } else {
                t.path().to_path_buf()
            };
            let root = Root::open(path.to_str().unwrap()).unwrap();
            fs::create_dir(path.join(kind.directory())).unwrap();
            fs::write(path.join(kind.directory()).join(name), b"x").unwrap();
            root.save(&Registry {
                entries: vec![entry(kind, name, Provider::Manual, None)],
                ..Registry::default()
            })
            .unwrap();
        }
        for op in ["disable", "disable", "enable", "enable", "remove"] {
            let mut pa = json!({"directory":a.path(),"file":format!("{}/{name}",kind.directory())});
            let mut pb = json!({"directory":b.path(),"file":format!("{}/{name}",kind.directory())});
            if kind == Kind::Datapack {
                pa["world"] = json!("World");
                pb["world"] = json!("World");
            }
            let method = format!("{}.{op}", kind.directory());
            assert_eq!(rust(&method, Some(pa)), zig(&method, Some(pb)), "{method}");
            let rel = if kind == Kind::Datapack {
                "saves/World/.jmcl/content.json"
            } else {
                ".jmcl/content.json"
            };
            assert_eq!(
                fs::read(a.path().join(rel)).unwrap(),
                fs::read(b.path().join(rel)).unwrap()
            );
        }
    }
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn unknown_fields_and_registry_corruption_match() {
    let t = tempfile::tempdir().unwrap();
    same(
        "mods.list",
        Some(json!({"directory":t.path(),"world":123,"file":false,"project":[]})),
    );
    same(
        "mods.remove",
        Some(json!({"directory":t.path(),"world":123,"file":"mods/a.jar"})),
    );
    fs::create_dir_all(t.path().join(".jmcl")).unwrap();
    for raw in [
        "[]",
        "null",
        "{\"schema_version\":2,\"entries\":[]}",
        "{\"entries\":[{\"kind\":\"mod\",\"file\":\"../outside\",\"sha1\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"size\":1,\"source\":{\"provider\":\"manual\"}}]}",
    ] {
        fs::write(t.path().join(".jmcl/content.json"), raw).unwrap();
        same("mods.list", Some(json!({"directory":t.path()})));
    }
}
#[test]
#[cfg(unix)]
#[ignore = "requires pinned Zig executable"]
fn configured_registry_and_content_read_aliases_match() {
    let t = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join(".jmcl")).unwrap();
    fs::write(
        external.path().join("registry.json"),
        b"{\"schema_version\":1,\"entries\":[]}\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("registry.json"),
        t.path().join(".jmcl/content.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink(external.path(), t.path().join("mods")).unwrap();
    fs::write(external.path().join("unmanaged.jar"), b"x").unwrap();
    same("mods.list", Some(json!({"directory":t.path()})));
    assert_eq!(
        fs::read(external.path().join("registry.json")).unwrap(),
        b"{\"schema_version\":1,\"entries\":[]}\n"
    );
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn anchored_directory_alias_mutations_match() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let xa = tempfile::tempdir().unwrap();
    let xb = tempfile::tempdir().unwrap();
    for (root, target) in [(&a, &xa), (&b, &xb)] {
        fs::write(target.path().join("a.jar"), b"x").unwrap();
        fs::write(target.path().join("keep"), b"untouched").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(target.path(), root.path().join("mods")).unwrap();
        #[cfg(windows)]
        {
            assert!(
                Command::new("cmd")
                    .args(["/C", "mklink", "/J"])
                    .arg(root.path().join("mods"))
                    .arg(target.path())
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        Root::open(root.path().to_str().unwrap())
            .unwrap()
            .save(&Registry {
                entries: vec![entry(Kind::Mod, "a.jar", Provider::Manual, None)],
                ..Registry::default()
            })
            .unwrap();
    }
    for op in ["list", "disable", "enable", "remove"] {
        let method = format!("mods.{op}");
        let pa = json!({"directory":a.path(),"file":"mods/a.jar"});
        let pb = json!({"directory":b.path(),"file":"mods/a.jar"});
        assert_eq!(rust(&method, Some(pa)), zig(&method, Some(pb)));
        assert_eq!(fs::read(xa.path().join("keep")).unwrap(), b"untouched");
        assert_eq!(fs::read(xb.path().join("keep")).unwrap(), b"untouched");
    }
    assert!(!xa.path().join("a.jar").exists());
    assert!(!xb.path().join("a.jar").exists());
}

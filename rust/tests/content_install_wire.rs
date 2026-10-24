use jmcl_core_rust::{
    content::{curseforge, modrinth, remote, rpc},
    install,
    store::Store,
};
use serde_json::{json, value::RawValue};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
struct Fixture(HashMap<String, Vec<u8>>);
impl remote::Metadata for Fixture {
    fn get(&self, u: &str, _: usize) -> modrinth::Result<Vec<u8>> {
        self.0
            .get(u)
            .cloned()
            .ok_or_else(|| modrinth::Error("MissingFixture".into()))
    }
}
impl curseforge::Metadata for Fixture {
    fn get(&self, u: &str, key: &str, _: usize) -> modrinth::Result<Vec<u8>> {
        assert_eq!(key, "synthetic-key");
        assert!(u.starts_with("https://api.curseforge.com/"));
        remote::Metadata::get(self, u, 0)
    }
}
impl install::Transport for Fixture {
    fn fetch(&self, _: &str, _: &mut dyn Write) -> Result<u16, install::Error> {
        panic!("fixture files must be already verified in CAS")
    }
}
fn docs(kind: &str, provider: &str) -> Fixture {
    let hash = format!("{:x}", Sha1::digest(b"content"));
    let ext = if kind == "mods" { "jar" } else { "zip" };
    let mut map = HashMap::new();
    if provider == "modrinth" {
        let ptype = match kind {
            "mods" => "mod",
            "resourcepacks" => "resourcepack",
            "shaderpacks" => "shader",
            _ => "datapack",
        };
        let loader = match kind {
            "mods" => "fabric",
            "resourcepacks" => "minecraft",
            _ => "datapack",
        };
        let project = json!({"id":"Proj1","slug":"project","project_type":ptype});
        let version = json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":[loader],"files":[{"url":"https://cdn.invalid/content","filename":format!("one.{ext}"),"size":7,"hashes":{"sha1":hash}}]});
        map.insert(
            modrinth::project_url("project").unwrap(),
            serde_json::to_vec(&project).unwrap(),
        );
        map.insert(
            modrinth::versions_url(
                "Proj1",
                "1.20.1",
                if kind == "shaderpacks" {
                    None
                } else {
                    Some(loader)
                },
            )
            .unwrap(),
            serde_json::to_vec(&json!([version])).unwrap(),
        );
    } else {
        let class = match kind {
            "mods" => 6,
            "resourcepacks" => 12,
            "shaderpacks" => 6552,
            _ => 5290,
        };
        map.insert(
            "https://api.curseforge.com/v1/mods/42".into(),
            serde_json::to_vec(
                &json!({"data":{"id":42,"slug":"project","name":"Project","classId":class}}),
            )
            .unwrap(),
        );
        let f = json!({"id":1,"modId":42,"fileName":format!("one.{ext}"),"fileLength":7,"hashes":[{"algo":1,"value":hash}],"releaseType":1,"fileDate":"2024","gameVersions":["1.20.1","Fabric"],"downloadUrl":"https://cdn.invalid/content"});
        map.insert(
            curseforge::page_url(
                "42",
                "1.20.1",
                if kind == "mods" { Some("fabric") } else { None },
                0,
            )
            .unwrap(),
            serde_json::to_vec(&json!({"data":[f]})).unwrap(),
        );
    }
    Fixture(map)
}
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, p: &Path, v: &mut Vec<(String, Vec<u8>)>) {
        if !p.exists() {
            return;
        }
        for e in std::fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, v)
            } else {
                v.push((
                    p.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    std::fs::read(p).unwrap(),
                ))
            }
        }
    }
    let mut v = vec![];
    walk(root, root, &mut v);
    v.sort();
    v
}
#[test]
#[ignore = "requires explicitly injected pinned JMCL_CONTENT_SERVICE_ORACLE"]
fn injected_service_raw_wire_and_disk() {
    let zig = std::env::var_os("JMCL_CONTENT_SERVICE_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for provider in ["modrinth", "curseforge"] {
            for mode in ["success", "skip", "invalid_project", "missing_metadata"] {
                let t = tempfile::tempdir().unwrap();
                let game = t.path().join("game");
                let cache = t.path().join("cache");
                let fixture = docs(family, provider);
                let fixture_path = t.path().join("fixtures.json");
                let serial: HashMap<_, _> = fixture
                    .0
                    .iter()
                    .map(|(k, v)| (k, String::from_utf8(v.clone()).unwrap()))
                    .collect();
                std::fs::write(&fixture_path, serde_json::to_vec(&serial).unwrap()).unwrap();
                let hash = format!("{:x}", Sha1::digest(b"content"));
                let mut p = json!({"directory":game,"minecraft_version":"1.20.1","project":if provider=="modrinth"{"project"}else{"42"},"provider":provider,"store_directory":cache,"workers":1,"retries":0});
                if family == "mods" {
                    p["loader"] = json!("fabric")
                }
                if family == "datapacks" {
                    p["world"] = json!("World")
                }
                if provider == "curseforge" {
                    p["api_key"] = json!("synthetic-key")
                }
                if mode == "invalid_project" {
                    p["project"] = json!("bad/id")
                }
                if mode == "missing_metadata" {
                    p["project"] = json!("Missing")
                }
                let request = json!({"protocol":1,"id":"fixture","method":format!("{family}.install"),"params":p});
                let mut results = vec![];
                for is_zig in [true, false] {
                    std::fs::create_dir_all(if family == "datapacks" {
                        game.join("saves/World")
                    } else {
                        game.clone()
                    })
                    .unwrap();
                    let store = Store::open(&cache).unwrap();
                    let source = t.path().join("source");
                    if source.exists() {
                        std::fs::remove_file(&source).unwrap();
                    }
                    std::fs::write(&source, b"content").unwrap();
                    store.import_verified(&source, &hash, 7).unwrap();
                    drop(store);
                    let invoke = || -> Vec<u8> {
                        if is_zig {
                            let mut c = Command::new(&zig)
                                .arg("rpc")
                                .env("JMCL_CONTENT_FIXTURES", &fixture_path)
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .stderr(Stdio::piped())
                                .spawn()
                                .unwrap();
                            writeln!(c.stdin.take().unwrap(), "{request}").unwrap();
                            let o = c.wait_with_output().unwrap();
                            assert!(o.status.success(), "{:?}", o.stderr);
                            o.stdout
                        } else {
                            let raw =
                                RawValue::from_string(serde_json::to_string(&p).unwrap()).unwrap();
                            let mut out = vec![];
                            rpc::handle_with(
                                &format!("{family}.install"),
                                Some(&raw),
                                "fixture",
                                &mut out,
                                &fixture,
                                &fixture,
                            )
                            .unwrap()
                            .unwrap();
                            out
                        }
                    };
                    if mode == "skip" {
                        let first = invoke();
                        assert!(String::from_utf8_lossy(&first).contains("\"event\":\"result\""));
                    }
                    let output = invoke();
                    if mode == "success" || mode == "skip" {
                        assert!(
                            String::from_utf8_lossy(&output).contains("\"event\":\"result\""),
                            "{}",
                            String::from_utf8_lossy(&output)
                        );
                    }
                    results.push((output, snapshot(&game)));
                    std::fs::remove_dir_all(&game).unwrap();
                }
                assert_eq!(results[0], results[1], "{family} {provider} {mode}");
                count += 1;
            }
        }
    }
    eprintln!("{count} injected raw content install wire/disk cases matched");
}
#[test]
fn writer_failure_aborts_before_registry_publication() {
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("fixture writer failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let t = tempfile::tempdir().unwrap();
    let game = t.path().join("game");
    let cache = t.path().join("cache");
    std::fs::create_dir(&game).unwrap();
    let source = t.path().join("source");
    std::fs::write(&source, b"content").unwrap();
    let hash = format!("{:x}", Sha1::digest(b"content"));
    Store::open(&cache)
        .unwrap()
        .import_verified(&source, &hash, 7)
        .unwrap();
    let p=RawValue::from_string(json!({"directory":game,"minecraft_version":"1.20.1","project":"project","loader":"fabric","store_directory":cache,"workers":1}).to_string()).unwrap();
    let f = docs("mods", "modrinth");
    assert!(
        rpc::handle_with("mods.install", Some(&p), "fixture", &mut Fail, &f, &f)
            .unwrap()
            .is_err()
    );
    assert!(!game.join(".jmcl/content.json").exists());
}
#[test]
#[ignore = "requires unmodified pinned JMCL_ZIG_ORACLE"]
fn original_binary_pre_network_errors() {
    let zig = std::env::var_os("JMCL_ZIG_ORACLE").unwrap();
    let f = Fixture(HashMap::new());
    let mut count = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        let base = json!({"directory":"","minecraft_version":"1.20.1","project":"project","loader":if family=="mods"{json!("fabric")}else{serde_json::Value::Null},"world":"World"});
        let mut cases = vec![json!({}), json!(null), json!([]), base.clone()];
        for (key, value) in [
            ("directory", json!(7)),
            ("directory", json!([])),
            ("api_key", json!([115, 121, 110, 116, 104])),
            ("workers", json!(0)),
            ("workers", json!(33)),
            ("workers", json!(256)),
            ("retries", json!(6)),
            ("with_dependencies", json!(null)),
            ("api_key", json!(7)),
            ("project", json!(null)),
            ("store_directory", json!(null)),
        ] {
            let mut p = base.clone();
            p[key] = value;
            cases.push(p)
        }
        for p in cases {
            let req = json!({"protocol":1,"id":"negative","method":format!("{family}.install"),"params":p});
            let mut c = Command::new(&zig)
                .arg("rpc")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            writeln!(c.stdin.take().unwrap(), "{req}").unwrap();
            let o = c.wait_with_output().unwrap();
            let raw = RawValue::from_string(p.to_string()).unwrap();
            let mut out = vec![];
            rpc::handle_with(
                &format!("{family}.install"),
                Some(&raw),
                "negative",
                &mut out,
                &f,
                &f,
            )
            .unwrap()
            .unwrap();
            assert_eq!(o.stdout, out, "{family} {p}");
            count += 1
        }
    }
    eprintln!("{count} unmodified oracle pre-network errors matched");
}
fn invoke(
    zig: &std::ffi::OsStr,
    fixture_path: &Path,
    f: &Fixture,
    req: &serde_json::Value,
    is_zig: bool,
) -> Vec<u8> {
    if is_zig {
        let mut c = Command::new(zig)
            .arg("rpc")
            .env("JMCL_CONTENT_FIXTURES", fixture_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(c.stdin.take().unwrap(), "{req}").unwrap();
        let o = c.wait_with_output().unwrap();
        assert!(o.status.success());
        o.stdout
    } else {
        let raw = RawValue::from_string(req["params"].to_string()).unwrap();
        let mut out = vec![];
        rpc::handle_with(
            req["method"].as_str().unwrap(),
            Some(&raw),
            req["id"].as_str().unwrap(),
            &mut out,
            f,
            f,
        )
        .unwrap()
        .unwrap();
        out
    }
}
#[test]
#[ignore = "requires explicitly injected pinned JMCL_CONTENT_SERVICE_ORACLE"]
fn injected_version_switch_wire_and_disk() {
    let zig = std::env::var_os("JMCL_CONTENT_SERVICE_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for provider in ["modrinth", "curseforge"] {
            for mode in ["enabled", "disabled", "same", "missing", "incompatible"] {
                let t = tempfile::tempdir().unwrap();
                let game = t.path().join("game");
                let cache = t.path().join("cache");
                let fixture_path = t.path().join("fixtures.json");
                let mut f = docs(family, provider);
                let hash = format!("{:x}", Sha1::digest(b"content"));
                let loader = match family {
                    "mods" => "fabric",
                    "resourcepacks" => "minecraft",
                    _ => "datapack",
                };
                let ext = if family == "mods" { "jar" } else { "zip" };
                let version = if provider == "modrinth" { "Ver2" } else { "2" };
                if provider == "modrinth" {
                    let v = json!({"id":"Ver2","project_id":"Proj1","version_number":"2","version_type":"release","game_versions":[if mode=="incompatible"{"other"}else{"1.20.1"}],"loaders":[loader],"files":[{"filename":format!("new.{ext}"),"url":"https://cdn.invalid/new","size":7,"hashes":{"sha1":hash}}]});
                    f.0.insert(
                        modrinth::version_url("Ver2").unwrap(),
                        serde_json::to_vec(&v).unwrap(),
                    );
                } else {
                    let v = json!({"data":{"id":2,"modId":42,"fileName":format!("new.{ext}"),"fileLength":7,"hashes":[{"value":hash,"algo":1}],"releaseType":1,"fileDate":"2025","gameVersions":[if mode=="incompatible"{"other"}else{"1.20.1"},"Fabric"],"downloadUrl":"https://cdn.invalid/new"}});
                    f.0.insert(
                        "https://api.curseforge.com/v1/mods/42/files/2".into(),
                        serde_json::to_vec(&v).unwrap(),
                    );
                }
                let serial: HashMap<_, _> =
                    f.0.iter()
                        .map(|(k, v)| (k, String::from_utf8(v.clone()).unwrap()))
                        .collect();
                std::fs::write(&fixture_path, serde_json::to_vec(&serial).unwrap()).unwrap();
                let mut p = json!({"directory":game,"minecraft_version":"1.20.1","project":if provider=="modrinth"{"project"}else{"42"},"provider":provider,"store_directory":cache,"workers":1,"retries":0});
                if family == "mods" {
                    p["loader"] = json!("fabric")
                }
                if family == "datapacks" {
                    p["world"] = json!("World")
                }
                if provider == "curseforge" {
                    p["api_key"] = json!("synthetic-key")
                }
                let mut results = vec![];
                let mut readonly_states = vec![];
                for is_zig in [true, false] {
                    let target = if family == "datapacks" {
                        game.join("saves/World")
                    } else {
                        game.clone()
                    };
                    std::fs::create_dir_all(&target).unwrap();
                    let source = t.path().join(if is_zig { "source-z" } else { "source-r" });
                    std::fs::write(&source, b"content").unwrap();
                    Store::open(&cache)
                        .unwrap()
                        .import_verified(&source, &hash, 7)
                        .unwrap();
                    let install = json!({"protocol":1,"id":"setup","method":format!("{family}.install"),"params":p});
                    let o = invoke(&zig, &fixture_path, &f, &install, is_zig);
                    assert!(String::from_utf8_lossy(&o).contains("\"event\":\"result\""));
                    if mode == "disabled" {
                        jmcl_core_rust::content::Root::open(target.to_str().unwrap())
                            .unwrap()
                            .toggle(
                                jmcl_core_rust::content::registry::Kind::from_directory(family)
                                    .unwrap(),
                                Some("project"),
                                None,
                                false,
                            )
                            .unwrap();
                    }
                    let mut params = p.clone();
                    params["version_id"] = json!(if mode == "same" {
                        if provider == "modrinth" { "Ver1" } else { "1" }
                    } else {
                        version
                    });
                    if mode == "missing" {
                        params["project"] = json!("missing")
                    };
                    let req = json!({"protocol":1,"id":"switch","method":format!("{family}.set-version"),"params":params});
                    let o = invoke(&zig, &fixture_path, &f, &req, is_zig);
                    let disk = snapshot(&game);
                    readonly_states.push(
                        disk.iter()
                            .map(|(path, _)| {
                                (
                                    path.clone(),
                                    std::fs::metadata(game.join(path))
                                        .map(|metadata| metadata.permissions().readonly()),
                                )
                            })
                            .collect::<Vec<_>>(),
                    );
                    results.push((o, disk));
                    std::fs::remove_dir_all(&game).unwrap();
                }
                // Run both implementations before asserting success so a native
                // failure identifies its side and preserves both disk outcomes.
                // These are synthetic fixture bytes, never user credentials.
                if matches!(mode, "enabled" | "disabled" | "same") {
                    for (index, (output, _)) in results.iter().enumerate() {
                        assert!(
                            String::from_utf8_lossy(output).contains("\"event\":\"result\""),
                            "{family} {provider} {mode}: {} failed\nZig wire: {}\nRust wire: {}\nZig disk: {:?}\nRust disk: {:?}\nReadonly states [Zig, Rust]: {:?}",
                            if index == 0 {
                                "injected pinned Zig"
                            } else {
                                "Rust"
                            },
                            String::from_utf8_lossy(&results[0].0),
                            String::from_utf8_lossy(&results[1].0),
                            results[0].1,
                            results[1].1,
                            readonly_states,
                        );
                    }
                }
                assert_eq!(results[0], results[1], "{family} {provider} {mode}");
                count += 1;
            }
        }
    }
    eprintln!("{count} injected content switch raw wire/disk cases matched");
}
#[test]
#[ignore = "requires explicitly injected pinned JMCL_CONTENT_SERVICE_ORACLE"]
fn injected_cli_install_and_switch() {
    let zig = std::env::var_os("JMCL_CONTENT_SERVICE_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for provider in ["modrinth", "curseforge"] {
            for op in ["install", "set-version"] {
                for mode in [
                    "success",
                    "no_args",
                    "missing_project",
                    "bad_flag",
                    "bad_workers",
                    "without_loader",
                ] {
                    let t = tempfile::tempdir().unwrap();
                    let root = t.path().join("instances");
                    let game = root.join("demo/.minecraft");
                    let cache = t.path().join("cache");
                    let fixture_path = t.path().join("fixtures.json");
                    let mut f = docs(family, provider);
                    add_next_version(&mut f, family, provider);
                    let serial: HashMap<_, _> =
                        f.0.iter()
                            .map(|(k, v)| (k, String::from_utf8(v.clone()).unwrap()))
                            .collect();
                    std::fs::write(&fixture_path, serde_json::to_vec(&serial).unwrap()).unwrap();
                    let hash = format!("{:x}", Sha1::digest(b"content"));
                    let project = if provider == "modrinth" {
                        "project"
                    } else {
                        "42"
                    };
                    let mut args: Vec<String> = vec![
                        "demo".into(),
                        "--project".into(),
                        project.into(),
                        "--instances-directory".into(),
                        root.to_str().unwrap().into(),
                        "--store-directory".into(),
                        cache.to_str().unwrap().into(),
                        "--workers".into(),
                        "1".into(),
                        "--retries".into(),
                        "0".into(),
                    ];
                    if family == "datapacks" {
                        args.extend(["--world".into(), "World".into()])
                    }
                    if op == "install" {
                        args.extend(["--provider".into(), provider.into()])
                    } else {
                        args.extend([
                            "--version".into(),
                            if provider == "modrinth" {
                                "Ver2".into()
                            } else {
                                "2".into()
                            },
                        ])
                    }
                    if provider == "curseforge" {
                        args.extend(["--api-key".into(), "synthetic-key".into()])
                    }
                    match mode {
                        "no_args" => args.clear(),
                        "missing_project" => {
                            args.drain(1..3);
                        }
                        "bad_flag" => args.extend(["--unknown".into(), "x".into()]),
                        "bad_workers" => args.extend(["--workers".into(), "0".into()]),
                        _ => (),
                    }
                    let mut results = vec![];
                    for is_zig in [true, false] {
                        let target = if family == "datapacks" {
                            game.join("saves/World")
                        } else {
                            game.clone()
                        };
                        std::fs::create_dir_all(&target).unwrap();
                        std::fs::write(root.join("demo/instance.json"),json!({"schema_version":1,"id":"demo","name":"Demo","version_id":"1.20.1","fabric_loader":if mode=="without_loader"{serde_json::Value::Null}else{json!("0.15.0")},"source":"official"}).to_string()).unwrap();
                        let source = t.path().join(if is_zig { "src-z" } else { "src-r" });
                        std::fs::write(&source, b"content").unwrap();
                        Store::open(&cache)
                            .unwrap()
                            .import_verified(&source, &hash, 7)
                            .unwrap();
                        if op == "set-version" {
                            let mut p = json!({"directory":game,"world":"World","minecraft_version":"1.20.1","project":project,"provider":provider,"store_directory":cache,"workers":1});
                            if family == "mods" {
                                p["loader"] = json!("fabric")
                            }
                            if provider == "curseforge" {
                                p["api_key"] = json!("synthetic-key")
                            }
                            let setup = json!({"protocol":1,"id":"setup","method":format!("{family}.install"),"params":p});
                            let o = invoke(&zig, &fixture_path, &f, &setup, is_zig);
                            assert!(String::from_utf8_lossy(&o).contains("\"event\":\"result\""));
                        }
                        let result = if is_zig {
                            let o = Command::new(&zig)
                                .args(["instance", family, op])
                                .args(&args)
                                .env("JMCL_CONTENT_FIXTURES", &fixture_path)
                                .output()
                                .unwrap();
                            (o.status.code().unwrap() as u8, o.stdout, o.stderr)
                        } else {
                            let mut out = vec![];
                            let mut err = vec![];
                            let code = jmcl_core_rust::content::cli_remote::run_with(
                                jmcl_core_rust::content::registry::Kind::from_directory(family)
                                    .unwrap(),
                                op,
                                &args,
                                &mut out,
                                &mut err,
                                &f,
                                &f,
                            )
                            .unwrap();
                            (code, out, err)
                        };
                        if mode == "success" {
                            assert_eq!(result.0, 0, "{family} {provider} {op} {:?}", result)
                        }
                        results.push((result, snapshot(&game)));
                        std::fs::remove_dir_all(&root).unwrap();
                    }
                    assert_eq!(results[0], results[1], "{family} {provider} {op} {mode}");
                    count += 1;
                }
            }
        }
    }
    eprintln!("{count} injected raw content CLI/disk cases matched");
}

fn add_next_version(f: &mut Fixture, family: &str, provider: &str) {
    let ext = if family == "mods" { "jar" } else { "zip" };
    if provider == "modrinth" {
        let b = f.0.iter().find(|(k, _)| k.contains("/version?")).unwrap().1;
        let mut v: serde_json::Value = serde_json::from_slice(b).unwrap();
        let mut v = v[0].take();
        v["id"] = json!("Ver2");
        v["files"][0]["filename"] = json!(format!("two.{ext}"));
        f.0.insert(
            modrinth::version_url("Ver2").unwrap(),
            serde_json::to_vec(&v).unwrap(),
        );
    } else {
        let b = f.0.iter().find(|(k, _)| k.contains("/files?")).unwrap().1;
        let mut v: serde_json::Value = serde_json::from_slice(b).unwrap();
        let mut v = v["data"][0].take();
        v["id"] = json!(2);
        v["fileName"] = json!(format!("two.{ext}"));
        f.0.insert(
            "https://api.curseforge.com/v1/mods/42/files/2".into(),
            serde_json::to_vec(&json!({"data":v})).unwrap(),
        );
    }
}

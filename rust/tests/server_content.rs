#![cfg(target_os = "linux")]
use jmcl_core_rust::{
    content::{
        Root,
        registry::{Entry, Kind, Provider, Registry, Source},
    },
    server_content,
    store::Store,
};
use serde_json::{Value, json, value::RawValue};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn invoke(method: &str, p: Option<Value>, oracle: Option<&str>) -> Vec<u8> {
    if let Some(oracle) = oracle {
        let mut req = json!({"protocol":1,"id":"c","method":method});
        if let Some(p) = p {
            req["params"] = p;
        }
        let mut child = Command::new(oracle)
            .arg("rpc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{req}").unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "{:?}", result.stderr);
        result.stdout
    } else {
        let raw = p.map(|p| RawValue::from_string(p.to_string()).unwrap());
        let mut out = vec![];
        server_content::handle(method, raw.as_deref(), "c", &mut out)
            .unwrap()
            .unwrap();
        out
    }
}
fn same(method: &str, p: Option<Value>) {
    let zig = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    assert_eq!(
        String::from_utf8(invoke(method, p.clone(), None)).unwrap(),
        String::from_utf8(invoke(method, p.clone(), Some(&zig))).unwrap(),
        "{method} {p:?}"
    );
}
fn fixture(base: &Path, fabric: bool, world: bool) -> std::path::PathBuf {
    let root = base.join("servers");
    let dir = root.join("test");
    fs::create_dir_all(&dir).unwrap();
    let mut m = json!({"id":"test","name":"Test","version_id":"1.20.1"});
    if fabric {
        m["fabric_loader"] = json!("0.16");
    }
    fs::write(dir.join("server.json"), m.to_string()).unwrap();
    fs::write(dir.join("server.properties"), b"level-name=World\n").unwrap();
    if world {
        fs::create_dir_all(dir.join("World")).unwrap();
    }
    root
}
#[test]
#[ignore = "requires unmodified pinned Zig executable; synthetic data only"]
fn raw_parameter_and_target_errors() {
    let t = tempfile::tempdir().unwrap();
    let root = fixture(t.path(), false, false);
    let mut count = 0;
    for family in ["mods", "datapacks"] {
        for op in [
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ] {
            let method = format!("server.{family}.{op}");
            for p in [
                None,
                Some(json!(null)),
                Some(json!({})),
                Some(json!([])),
                Some(
                    json!({"directory":root,"id":"test","project":"bad/id","version_id":"bad/id","store_directory":t.path().join("store")}),
                ),
                Some(
                    json!({"directory":root,"id":"missing","project":"x","version_id":"v","store_directory":t.path().join("store")}),
                ),
                Some(json!({"directory":"","id":"test","project":"x","version_id":"v"})),
                Some(json!({"directory":root,"id":"test","project":true,"version_id":12})),
                Some(
                    json!({"directory":root,"id":"test","file":"../unsafe","store_directory":t.path().join("store")}),
                ),
            ] {
                same(&method, p);
                count += 1;
            }
        }
    }
    eprintln!("{count} raw parameter/target comparisons");
}
#[test]
#[ignore = "requires unmodified pinned Zig executable; synthetic registries only"]
fn registry_mutations_and_base_world_paths() {
    let zig = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "datapacks"] {
        for op in ["list", "enable", "disable", "remove", "adopt"] {
            for selector in ["file", "project"] {
                for fabric in [false, true] {
                    let t = tempfile::tempdir().unwrap();
                    let mut result = vec![];
                    for oracle in [Some(zig.as_str()), None] {
                        let root = fixture(t.path(), fabric, true);
                        let dir = root.join("test");
                        let target = if family == "mods" {
                            dir.clone()
                        } else {
                            dir.join("World")
                        };
                        let kind = if family == "mods" {
                            Kind::Mod
                        } else {
                            Kind::Datapack
                        };
                        let file = format!(
                            "{}/test.{}",
                            kind.directory(),
                            if family == "mods" { "jar" } else { "zip" }
                        );
                        fs::create_dir_all(target.join(kind.directory())).unwrap();
                        fs::write(target.join(&file), b"benign").unwrap();
                        let hash = format!("{:x}", Sha1::digest(b"benign"));
                        let entry = Entry {
                            kind,
                            file: file.clone(),
                            sha1: hash,
                            size: 6,
                            enabled: true,
                            as_dependency: false,
                            source: Source {
                                provider: Provider::Modrinth,
                                project_id: Some("Proj1".into()),
                                slug: Some("test-project".into()),
                                version_id: Some("Ver1".into()),
                            },
                        };
                        if op != "adopt" {
                            Root::open(target.to_str().unwrap())
                                .unwrap()
                                .save(&Registry {
                                    schema_version: 1,
                                    entries: vec![entry],
                                })
                                .unwrap();
                        }
                        let mut p = json!({"directory":root,"id":"test","store_directory":t.path().join("cache")});
                        if matches!(op, "enable" | "disable" | "remove") {
                            p[selector] = json!(if selector == "file" {
                                file.as_str()
                            } else {
                                "test-project"
                            });
                        }
                        let bytes = invoke(&format!("server.{family}.{op}"), Some(p), oracle);
                        let registry = fs::read(target.join(".jmcl/content.json")).ok();
                        let present = target.join(&file).exists();
                        let disabled = target.join(format!("{file}.disabled")).exists();
                        if op == "adopt" {
                            use std::os::unix::fs::MetadataExt;
                            let store = Store::open(t.path().join("cache")).unwrap();
                            let h = format!("{:x}", Sha1::digest(b"benign"));
                            let f = t.path().join("cache/objects/sha1").join(&h[..2]).join(&h);
                            let _ = store;
                            assert_eq!(
                                fs::metadata(f).unwrap().ino(),
                                fs::metadata(target.join(&file)).unwrap().ino()
                            );
                        }
                        result.push((bytes, registry, present, disabled));
                        fs::remove_dir_all(root).unwrap();
                    }
                    assert_eq!(
                        result[0], result[1],
                        "{family} {op} {selector} fabric={fabric}"
                    );
                    count += 1;
                }
            }
        }
    }
    eprintln!("{count} registry/raw-wire/hardlink comparisons");
}
#[test]
#[ignore = "requires unmodified pinned Zig executable; synthetic paths only"]
fn level_name_and_loader_ordering() {
    let t = tempfile::tempdir().unwrap();
    let root = fixture(t.path(), false, false);
    for name in [
        "",
        "../outside",
        ".hidden",
        "World",
        "Other.World",
        "A B",
        "trailing.",
    ] {
        fs::write(
            root.join("test/server.properties"),
            format!("level-name={name}\n"),
        )
        .unwrap();
        for op in ["list", "remove", "adopt"] {
            let p = json!({"directory":root,"id":"test","file":"datapacks/x.zip","store_directory":t.path().join("cache")});
            same(&format!("server.datapacks.{op}"), Some(p));
        }
    }
    fs::write(root.join("test/server.properties"), b"difficulty=easy\n").unwrap();
    same(
        "server.datapacks.list",
        Some(json!({"directory":root,"id":"test"})),
    );
    fs::create_dir_all(root.join("test/.jmcl/server.pid")).unwrap();
    same(
        "server.mods.install",
        Some(
            json!({"directory":root,"id":"test","project":"x","store_directory":t.path().join("cache")}),
        ),
    );
    same(
        "server.mods.remove",
        Some(json!({"directory":root,"id":"test","file":"mods/x.jar"})),
    );
}

mod provider_fixture {
    use super::*;
    use jmcl_core_rust::{
        content::{curseforge, modrinth, remote},
        install,
    };
    use std::collections::HashMap;
    pub(super) struct Fixture(pub(super) HashMap<String, Vec<u8>>);
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
    pub(super) fn docs(kind: &str, provider: &str) -> Fixture {
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
}
#[test]
#[ignore = "requires injected pinned service oracle; benign preverified CAS fixtures only"]
fn provider_install_wire_registry_and_hardlinks() {
    let zig = std::env::var("JMCL_CONTENT_SERVICE_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "datapacks"] {
        for provider in ["modrinth", "curseforge"] {
            for mode in ["success", "skip", "invalid_project", "missing_metadata"] {
                let t = tempfile::tempdir().unwrap();
                let f = provider_fixture::docs(family, provider);
                let fixture_path = t.path().join("fixtures.json");
                let serial: std::collections::HashMap<_, _> =
                    f.0.iter()
                        .map(|(k, v)| (k, String::from_utf8(v.clone()).unwrap()))
                        .collect();
                fs::write(&fixture_path, serde_json::to_vec(&serial).unwrap()).unwrap();
                let mut results = vec![];
                for is_zig in [true, false] {
                    let root = fixture(t.path(), family == "mods", false);
                    let target = if family == "mods" {
                        root.join("test")
                    } else {
                        root.join("test/World")
                    };
                    let hash = format!("{:x}", Sha1::digest(b"content"));
                    let cache = t.path().join("cache");
                    let source = t.path().join("source");
                    if source.exists() {
                        fs::remove_file(&source).unwrap();
                    }
                    fs::write(&source, b"content").unwrap();
                    Store::open(&cache)
                        .unwrap()
                        .import_verified(&source, &hash, 7)
                        .unwrap();
                    let mut p = json!({"directory":root,"id":"test","project":if provider=="modrinth"{"project"}else{"42"},"provider":provider,"store_directory":cache,"workers":1,"retries":0});
                    if provider == "curseforge" {
                        p["api_key"] = json!("synthetic-key");
                    }
                    if mode == "invalid_project" {
                        p["project"] = json!("bad/id");
                    }
                    if mode == "missing_metadata" {
                        p["project"] = json!("Missing");
                    }
                    let call = || {
                        let method = format!("server.{family}.install");
                        if is_zig {
                            let mut c = Command::new(&zig)
                                .arg("rpc")
                                .env("JMCL_CONTENT_FIXTURES", &fixture_path)
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .stderr(Stdio::piped())
                                .spawn()
                                .unwrap();
                            writeln!(
                                c.stdin.take().unwrap(),
                                "{}",
                                json!({"protocol":1,"id":"c","method":method,"params":p})
                            )
                            .unwrap();
                            let o = c.wait_with_output().unwrap();
                            assert!(o.status.success(), "{:?}", o.stderr);
                            o.stdout
                        } else {
                            let raw = RawValue::from_string(p.to_string()).unwrap();
                            let mut out = vec![];
                            server_content::handle_with(&method, Some(&raw), "c", &mut out, &f, &f)
                                .unwrap()
                                .unwrap();
                            out
                        }
                    };
                    if mode == "skip" {
                        call();
                    }
                    let out = call();
                    let registry = fs::read(target.join(".jmcl/content.json")).ok();
                    let dest = target.join(if family == "mods" {
                        "mods/one.jar"
                    } else {
                        "datapacks/one.zip"
                    });
                    if dest.exists() {
                        use std::os::unix::fs::MetadataExt;
                        assert_eq!(
                            fs::metadata(&dest).unwrap().ino(),
                            fs::metadata(cache.join("objects/sha1").join(&hash[..2]).join(&hash))
                                .unwrap()
                                .ino()
                        );
                        assert!(fs::metadata(&dest).unwrap().permissions().readonly());
                    }
                    results.push((out, registry, fs::read(dest).ok()));
                    fs::remove_dir_all(root).unwrap();
                }
                assert_eq!(results[0], results[1], "{family} {provider} {mode}");
                count += 1;
            }
        }
    }
    eprintln!("{count} injected-provider server install comparisons");
}
fn cli_invoke(
    args: &[String],
    oracle: Option<&str>,
    fixture: Option<(&provider_fixture::Fixture, &Path)>,
) -> (u8, Vec<u8>, Vec<u8>) {
    if let Some(exe) = oracle {
        let mut c = Command::new(exe);
        c.arg("server").args(args);
        if let Some((_, p)) = fixture {
            c.env("JMCL_CONTENT_FIXTURES", p);
        }
        let o = c.output().unwrap();
        (o.status.code().unwrap() as u8, o.stdout, o.stderr)
    } else {
        let (mut out, mut err) = (vec![], vec![]);
        let code = if let Some((f, _)) = fixture {
            server_content::cli_with(args, &mut out, &mut err, f, f)
                .unwrap()
                .unwrap()
        } else {
            server_content::cli(args, &mut out, &mut err)
                .unwrap()
                .unwrap()
        };
        (code, out, err)
    }
}
#[test]
#[ignore = "requires unmodified pinned Zig executable; local CLI only"]
fn cli_parameters_and_local_state() {
    let zig = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "datapacks"] {
        for op in [
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ] {
            for tail in [
                vec![],
                vec!["test"],
                vec!["../invalid"],
                vec!["test", "--unknown", "value"],
                vec!["test", "--workers", "0"],
                vec!["test", "--project", "bad/id"],
                vec!["test", "--file", "../bad"],
                vec!["test", "--project", "x", "--file", "mods/a.jar"],
                vec!["test", "--project", "x", "--provider", "curseforge"],
                vec!["test", "--project", "x", "--api-key", "synthetic-key"],
            ] {
                let t = tempfile::tempdir().unwrap();
                let root = fixture(t.path(), false, false);
                let mut args = vec![family.to_owned(), op.to_owned()];
                args.extend(tail.into_iter().map(str::to_owned));
                if args.len() > 2 {
                    args.extend([
                        "--servers-directory".into(),
                        root.to_string_lossy().into_owned(),
                    ]);
                    if matches!(op, "install" | "set-version" | "adopt") {
                        args.extend([
                            "--store-directory".into(),
                            t.path().join("cache").to_string_lossy().into_owned(),
                        ]);
                    }
                }
                assert_eq!(
                    cli_invoke(&args, None, None),
                    cli_invoke(&args, Some(&zig), None),
                    "{args:?}"
                );
                count += 1;
            }
        }
    }
    eprintln!("{count} CLI argument/target comparisons");
}
#[test]
#[ignore = "requires injected pinned service oracle; synthetic provider CLI and switching"]
fn provider_switching_and_cli() {
    use jmcl_core_rust::content::modrinth;
    let zig = std::env::var("JMCL_CONTENT_SERVICE_ORACLE").unwrap();
    let mut count = 0;
    for family in ["mods", "datapacks"] {
        for provider in ["modrinth", "curseforge"] {
            for mode in ["enabled", "disabled", "noop", "missing_key"] {
                for wire in [false, true] {
                    let t = tempfile::tempdir().unwrap();
                    let mut f = provider_fixture::docs(family, provider);
                    let ext = if family == "mods" { "jar" } else { "zip" };
                    if provider == "modrinth" {
                        let (_, bytes) = f.0.iter().find(|(k, _)| k.contains("/version?")).unwrap();
                        let a: Value = serde_json::from_slice(bytes).unwrap();
                        let mut v = a[0].clone();
                        v["id"] = json!("Ver2");
                        v["files"][0]["filename"] = json!(format!("two.{ext}"));
                        f.0.insert(
                            modrinth::version_url("Ver2").unwrap(),
                            serde_json::to_vec(&v).unwrap(),
                        );
                    } else {
                        let (_, bytes) = f.0.iter().find(|(k, _)| k.contains("/files?")).unwrap();
                        let a: Value = serde_json::from_slice(bytes).unwrap();
                        let mut v = a["data"][0].clone();
                        v["id"] = json!(2);
                        v["fileName"] = json!(format!("two.{ext}"));
                        f.0.insert(
                            "https://api.curseforge.com/v1/mods/42/files/2".into(),
                            serde_json::to_vec(&json!({"data":v})).unwrap(),
                        );
                    }
                    let path = t.path().join("fixture.json");
                    let serial: std::collections::HashMap<_, _> =
                        f.0.iter()
                            .map(|(k, v)| (k, String::from_utf8(v.clone()).unwrap()))
                            .collect();
                    fs::write(&path, serde_json::to_vec(&serial).unwrap()).unwrap();
                    let mut results = vec![];
                    for oracle in [Some(zig.as_str()), None] {
                        let root = fixture(t.path(), family == "mods", false);
                        let cache = t.path().join("cache");
                        let source = t.path().join("source");
                        if source.exists() {
                            fs::remove_file(&source).unwrap();
                        }
                        fs::write(&source, b"content").unwrap();
                        let hash = format!("{:x}", Sha1::digest(b"content"));
                        Store::open(&cache)
                            .unwrap()
                            .import_verified(&source, &hash, 7)
                            .unwrap();
                        let base = vec![
                            "--servers-directory".to_owned(),
                            root.to_string_lossy().into_owned(),
                            "--store-directory".into(),
                            cache.to_string_lossy().into_owned(),
                            "--workers".into(),
                            "1".into(),
                            "--retries".into(),
                            "0".into(),
                        ];
                        let project = if provider == "modrinth" {
                            "project"
                        } else {
                            "42"
                        };
                        let mut install = vec![
                            family.into(),
                            "install".into(),
                            "test".into(),
                            "--project".into(),
                            project.into(),
                            "--provider".into(),
                            provider.into(),
                        ];
                        if provider == "curseforge" {
                            install.extend(["--api-key".into(), "synthetic-key".into()]);
                        }
                        install.extend(base.clone());
                        let initial = cli_invoke(&install, oracle, Some((&f, &path)));
                        assert_eq!(initial.0, 0, "{:?}", initial);
                        let target = if family == "mods" {
                            root.join("test")
                        } else {
                            root.join("test/World")
                        };
                        let kind = if family == "mods" {
                            Kind::Mod
                        } else {
                            Kind::Datapack
                        };
                        let old = format!("{}/one.{ext}", kind.directory());
                        if mode == "disabled" {
                            Root::open(target.to_str().unwrap())
                                .unwrap()
                                .toggle(kind, None, Some(&old), false)
                                .unwrap();
                        }
                        let mut switch = vec![
                            family.into(),
                            "set-version".into(),
                            "test".into(),
                            "--project".into(),
                            project.into(),
                            "--version".into(),
                            if mode == "noop" {
                                if provider == "modrinth" { "Ver1" } else { "1" }
                            } else if provider == "modrinth" {
                                "Ver2"
                            } else {
                                "2"
                            }
                            .into(),
                        ];
                        switch.extend(base);
                        if provider == "curseforge" && mode != "missing_key" {
                            switch.extend(["--api-key".into(), "synthetic-key".into()]);
                        }
                        let changed = if !wire {
                            cli_invoke(&switch, oracle, Some((&f, &path)))
                        } else {
                            let mut p = json!({"directory":root,"id":"test","project":project,"version_id":switch[6],"store_directory":cache,"workers":1,"retries":0});
                            if provider == "curseforge" && mode != "missing_key" {
                                p["api_key"] = json!("synthetic-key");
                            }
                            let method = format!("server.{family}.set-version");
                            let bytes = if let Some(exe) = oracle {
                                let mut c = Command::new(exe)
                                    .arg("rpc")
                                    .env("JMCL_CONTENT_FIXTURES", &path)
                                    .stdin(Stdio::piped())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::piped())
                                    .spawn()
                                    .unwrap();
                                writeln!(
                                    c.stdin.take().unwrap(),
                                    "{}",
                                    json!({"protocol":1,"id":"c","method":method,"params":p})
                                )
                                .unwrap();
                                let o = c.wait_with_output().unwrap();
                                assert!(o.status.success());
                                o.stdout
                            } else {
                                let raw = RawValue::from_string(p.to_string()).unwrap();
                                let mut out = vec![];
                                server_content::handle_with(
                                    &method,
                                    Some(&raw),
                                    "c",
                                    &mut out,
                                    &f,
                                    &f,
                                )
                                .unwrap()
                                .unwrap();
                                out
                            };
                            (0, bytes, vec![])
                        };
                        let mut list = vec![
                            family.into(),
                            "list".into(),
                            "test".into(),
                            "--servers-directory".into(),
                            root.to_string_lossy().into_owned(),
                        ];
                        let text = cli_invoke(&list, oracle, Some((&f, &path)));
                        list.push("--json".into());
                        let json = cli_invoke(&list, oracle, Some((&f, &path)));
                        results.push((
                            initial,
                            changed,
                            text,
                            json,
                            fs::read(target.join(".jmcl/content.json")).unwrap(),
                        ));
                        fs::remove_dir_all(root).unwrap();
                    }
                    assert_eq!(results[0], results[1], "{family} {provider} {mode}");
                    count += 1;
                }
            }
        }
    }
    eprintln!("{count} RPC/CLI install/switch/list sequence comparisons");
}
#[test]
fn base_world_creation_and_no_dimension_mutation() {
    let t = tempfile::tempdir().unwrap();
    let root = fixture(t.path(), false, false);
    let p = json!({"directory":root,"id":"test","file":"datapacks/not-present.zip"});
    let b = invoke("server.datapacks.list", Some(p.clone()), None);
    assert!(String::from_utf8(b).unwrap().contains("\"entries\":[]"));
    assert!(!root.join("test/World").exists());
    let b = invoke("server.datapacks.remove", Some(p), None);
    assert!(String::from_utf8(b).unwrap().contains("CONTENT_NOT_FOUND"));
    assert!(root.join("test/World").is_dir());
    assert!(!root.join("test/World_nether").exists());
    assert!(!root.join("test/World_the_end").exists());
}
#[test]
fn stopped_guard_and_vanilla_install_precedence() {
    // A harmless shell fixture only supplies a matching process identity. No game,
    // Java, supervisor, socket, or server command is started.
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let identity = "0123456789abcdef0123456789abcdef";
    let mut child = Child(
        Command::new("sh")
            .args([
                "-c",
                "printf ready; read ignored",
                &format!("-Djmcl.server.identity={identity}"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    // Synchronize with exec and the shell builtin before publishing its identity.
    use std::io::Read;
    let mut ready = [0; 5];
    child
        .0
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();
    assert_eq!(&ready, b"ready");
    let t = tempfile::tempdir().unwrap();
    let root = fixture(t.path(), false, true);
    let dir = root.join("test");
    fs::create_dir_all(dir.join(".jmcl")).unwrap();
    fs::write(dir.join(".jmcl/server.pid"),json!({"schema_version":1,"pid":child.0.id(),"supervisor_pid":child.0.id(),"identity":identity,"java_path":"/synthetic/unused","started_at_ms":1}).to_string()).unwrap();
    for family in ["mods", "datapacks"] {
        for op in [
            "enable",
            "disable",
            "remove",
            "adopt",
            "set-version",
            "install",
        ] {
            let p = json!({"directory":root,"id":"test","file":format!("{family}/a.zip"),"project":"x","version_id":"v","store_directory":t.path().join("store")});
            let mut p = p;
            if matches!(op, "enable" | "disable" | "remove") {
                p.as_object_mut().unwrap().remove("project");
            }
            let response: Value =
                serde_json::from_slice(&invoke(&format!("server.{family}.{op}"), Some(p), None))
                    .unwrap();
            assert_eq!(
                response["error"]["code"],
                if family == "mods" && op == "install" {
                    "SERVER_LOADER_REQUIRED"
                } else {
                    "SERVER_ALREADY_RUNNING"
                },
                "{family}.{op} {response}"
            );
        }
    }
    let list = invoke(
        "server.mods.list",
        Some(json!({"directory":root,"id":"test"})),
        None,
    );
    assert!(
        String::from_utf8(list)
            .unwrap()
            .contains("\"event\":\"result\"")
    );
    let _ = child.0.stdin.take();
}
#[test]
fn failed_output_prevents_server_registry_publication() {
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let t = tempfile::tempdir().unwrap();
    let root = fixture(t.path(), true, false);
    let f = provider_fixture::docs("mods", "modrinth");
    let cache = t.path().join("cache");
    let source = t.path().join("source");
    fs::write(&source, b"content").unwrap();
    Store::open(&cache)
        .unwrap()
        .import_verified(&source, &format!("{:x}", Sha1::digest(b"content")), 7)
        .unwrap();
    let p=RawValue::from_string(json!({"directory":root,"id":"test","project":"project","store_directory":cache,"workers":1,"retries":0}).to_string()).unwrap();
    assert!(
        server_content::handle_with("server.mods.install", Some(&p), "c", &mut Fail, &f, &f)
            .unwrap()
            .is_err()
    );
    assert!(!root.join("test/.jmcl/content.json").exists());
}
#[test]
#[ignore = "requires pinned oracle; provider identity and CLI validation ordering"]
fn provider_selectors_and_invalid_key_ordering() {
    let zig = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let mut count = 0;
    for provider in [Provider::Modrinth, Provider::Curseforge] {
        for op in ["enable", "disable", "remove"] {
            for by_file in [true, false] {
                let t = tempfile::tempdir().unwrap();
                let mut result = vec![];
                for oracle in [Some(zig.as_str()), None] {
                    let root = fixture(t.path(), true, false);
                    let dir = root.join("test");
                    fs::create_dir_all(dir.join("mods")).unwrap();
                    fs::write(dir.join("mods/one.jar"), b"content").unwrap();
                    let r = Registry {
                        schema_version: 1,
                        entries: vec![Entry {
                            kind: Kind::Mod,
                            file: "mods/one.jar".into(),
                            sha1: format!("{:x}", Sha1::digest(b"content")),
                            size: 7,
                            enabled: true,
                            as_dependency: false,
                            source: Source {
                                provider,
                                project_id: Some("42".into()),
                                slug: Some("project".into()),
                                version_id: Some("1".into()),
                            },
                        }],
                    };
                    Root::open(dir.to_str().unwrap()).unwrap().save(&r).unwrap();
                    let mut p = json!({"directory":root,"id":"test"});
                    if by_file {
                        p["file"] = json!("mods/one.jar");
                    } else {
                        p["project"] = json!("project");
                    }
                    result.push((
                        invoke(&format!("server.mods.{op}"), Some(p), oracle),
                        fs::read(dir.join(".jmcl/content.json")).unwrap(),
                    ));
                    fs::remove_dir_all(root).unwrap();
                }
                assert_eq!(result[0], result[1]);
                count += 1;
            }
        }
    }
    for family in ["mods", "datapacks"] {
        let t = tempfile::tempdir().unwrap();
        let root = fixture(t.path(), true, true);
        let target = if family == "mods" {
            root.join("test")
        } else {
            root.join("test/World")
        };
        fs::create_dir_all(target.join(".jmcl")).unwrap();
        fs::write(target.join(".jmcl/content.json"), b"malformed-registry").unwrap();
        let args = vec![
            family.into(),
            "install".into(),
            "test".into(),
            "--servers-directory".into(),
            root.to_string_lossy().into_owned(),
            "--store-directory".into(),
            t.path().join("cache").to_string_lossy().into_owned(),
            "--provider".into(),
            "curseforge".into(),
            "--project".into(),
            "42".into(),
            "--api-key".into(),
            "invalid\nkey".into(),
        ];
        let f = provider_fixture::docs(family, "curseforge");
        let path = t.path().join("unused");
        assert_eq!(
            cli_invoke(&args, None, Some((&f, &path))),
            cli_invoke(&args, Some(&zig), None)
        );
        count += 1;
    }
    eprintln!("{count} provider-selector/validation ordering comparisons");
}

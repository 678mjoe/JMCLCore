use jmcl_core_rust::content::{
    self, Root,
    registry::{Entry, Kind, Provider, Registry, Source},
};
use std::{fs, process::Command};
fn oracle(args: &[String]) -> (i32, Vec<u8>, Vec<u8>) {
    let o = Command::new(std::env::var("JMCL_ZIG_ORACLE").expect("pinned oracle required"))
        .arg("instance")
        .args(args)
        .output()
        .unwrap();
    (o.status.code().unwrap(), o.stdout, o.stderr)
}
fn rust(args: &[String]) -> (i32, Vec<u8>, Vec<u8>) {
    let (mut out, mut err) = (vec![], vec![]);
    let code = content::cli(args, &mut out, &mut err).unwrap().unwrap();
    (code.into(), out, err)
}
fn same(args: Vec<String>) {
    assert_eq!(rust(&args), oracle(&args), "{args:?}")
}
fn args(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| s.to_string()).collect()
}
fn fixture(root: &std::path::Path, kind: Kind) {
    let game = root.join("demo/.minecraft");
    fs::create_dir_all(game.join(kind.directory())).unwrap();
    fs::write(root.join("demo/instance.json"),r#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"1.21.1","fabric_loader":null,"neoforge_version":null,"forge_version":null,"source":"official"}"#).unwrap();
    let game = if kind == Kind::Datapack {
        game.join("saves/World")
    } else {
        game
    };
    fs::create_dir_all(game.join(kind.directory())).unwrap();
    let name = if kind == Kind::Mod { "a.jar" } else { "a.zip" };
    fs::write(game.join(kind.directory()).join(name), b"x").unwrap();
    let r = Root::open(game.to_str().unwrap()).unwrap();
    r.save(&Registry {
        entries: vec![Entry {
            kind,
            file: format!("{}/{name}", kind.directory()),
            sha1: "a".repeat(40),
            size: 1,
            source: Source {
                provider: Provider::Manual,
                project_id: None,
                version_id: None,
                slug: None,
            },
            enabled: true,
            as_dependency: false,
        }],
        ..Registry::default()
    })
    .unwrap();
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn local_content_cli_usage_and_errors_match() {
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        same(args(&[family]));
        same(args(&[family, "unknown"]));
        for op in ["list", "remove", "enable", "disable"] {
            same(args(&[family, op]));
            same(args(&[family, op, "BAD"]));
            same(args(&[family, op, "demo", "--wat"]));
            same(args(&[family, op, "demo"]));
        }
    }
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn local_content_cli_success_and_mutation_match() {
    for kind in [
        Kind::Mod,
        Kind::Resourcepack,
        Kind::Shaderpack,
        Kind::Datapack,
    ] {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        fixture(a.path(), kind);
        fixture(b.path(), kind);
        let name = if kind == Kind::Mod { "a.jar" } else { "a.zip" };
        for op in ["list", "disable", "disable", "enable", "remove"] {
            let mut ar = args(&[
                kind.directory(),
                op,
                "demo",
                "--instances-directory",
                a.path().to_str().unwrap(),
            ]);
            let mut br = args(&[
                kind.directory(),
                op,
                "demo",
                "--instances-directory",
                b.path().to_str().unwrap(),
            ]);
            if kind == Kind::Datapack {
                ar.extend(args(&["--world", "World"]));
                br.extend(args(&["--world", "World"]));
            }
            if op != "list" {
                ar.extend(args(&["--file", &format!("{}/{name}", kind.directory())]));
                br.extend(args(&["--file", &format!("{}/{name}", kind.directory())]));
            }
            assert_eq!(rust(&ar), oracle(&br), "{ar:?}");
            if op == "list" {
                ar.push("--json".into());
                br.push("--json".into());
                assert_eq!(rust(&ar), oracle(&br));
            }
        }
    }
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn content_cli_corrupt_registries_match() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path(), Kind::Mod);
    for raw in [
        "",
        "[]",
        "null",
        "{}",
        "{\"entries\":[{}]}",
        "{\"schema_version\":2}",
        "{\"entries\":[{\"kind\":\"bad\"}]}",
    ] {
        fs::write(t.path().join("demo/.minecraft/.jmcl/content.json"), raw).unwrap();
        same(args(&[
            "mods",
            "list",
            "demo",
            "--instances-directory",
            t.path().to_str().unwrap(),
        ]));
    }
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn content_cli_invalid_selectors_match() {
    let t = tempfile::tempdir().unwrap();
    fixture(t.path(), Kind::Mod);
    for op in ["remove", "disable", "enable"] {
        for selector in [
            vec!["--project", "bad/ref"],
            vec!["--file", "../outside"],
            vec!["--file", "mods/CON.jar"],
        ] {
            let mut a = args(&[
                "mods",
                op,
                "demo",
                "--instances-directory",
                t.path().to_str().unwrap(),
            ]);
            a.extend(args(&selector));
            same(a);
        }
    }
}

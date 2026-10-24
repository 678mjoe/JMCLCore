use jmcl_core_rust::{
    content::{Root, registry::Kind},
    store::{Store, object_path},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
#[test]
fn adoption_reuses_real_file_identity_and_manual_registry() {
    let t = tempfile::tempdir().unwrap();
    let game = t.path().join("game");
    fs::create_dir_all(game.join("mods")).unwrap();
    fs::write(game.join("mods/one.jar"), b"content").unwrap();
    fs::write(game.join("mods/ignore.txt"), b"ignore").unwrap();
    let root = Root::open(game.to_str().unwrap()).unwrap();
    let store = Store::open(t.path().join("cache")).unwrap();
    let a = root.adopt(Kind::Mod, &store).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].size, 7);
    assert!(root.adopt(Kind::Mod, &store).unwrap().is_empty());
    let object = t
        .path()
        .join("cache")
        .join(object_path(&a[0].sha1).unwrap());
    assert_eq!(fs::read(&object).unwrap(), b"content");
    assert!(fs::metadata(&object).unwrap().permissions().readonly());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&object).unwrap().ino(),
            fs::metadata(game.join("mods/one.jar")).unwrap().ino()
        );
    }
    let r = root.load().unwrap();
    assert_eq!(
        r.entries[0].source.provider,
        jmcl_core_rust::content::registry::Provider::Manual
    );
    assert_eq!(fs::read(game.join("mods/ignore.txt")).unwrap(), b"ignore");
}
fn run(exe: &Path, args: &[&str], req: Option<&Value>, dir: &Path) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(exe)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(req) = req {
        writeln!(child.stdin.take().unwrap(), "{req}").unwrap()
    }
    let o = child.wait_with_output().unwrap();
    (o.status.code().unwrap(), o.stdout, o.stderr)
}
fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, p: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        if !p.exists() {
            return;
        }
        for e in fs::read_dir(p).unwrap() {
            let e = e.unwrap();
            if e.path().is_dir() {
                walk(base, &e.path(), out)
            } else {
                out.push((
                    e.path()
                        .strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(e.path()).unwrap(),
                ));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE"]
fn adopt_rpc_and_disk_match_oracle() {
    let zig = std::path::PathBuf::from(std::env::var_os("JMCL_ZIG_ORACLE").unwrap());
    let rust = Path::new(env!("CARGO_BIN_EXE_jmcl-core-rust"));
    let mut count = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for fixture in ["empty", "one", "zero", "corrupt", "directory", "alias"] {
            for params in [
                json!({"directory":"game","world":"World","store_directory":"cache"}),
                json!({"directory":"","world":"World","store_directory":"cache"}),
                json!({}),
                json!([]),
                json!({"directory":"game","world":null,"store_directory":"cache"}),
            ] {
                let mut results = vec![];
                for exe in [zig.as_path(), rust] {
                    let t = tempfile::tempdir().unwrap();
                    let game = t.path().join("game");
                    let target = if family == "datapacks" {
                        game.join("saves/World")
                    } else {
                        game.clone()
                    };
                    fs::create_dir_all(target.join(family)).unwrap();
                    let ext = if family == "mods" { "jar" } else { "zip" };
                    if fixture == "one" || fixture == "zero" {
                        fs::write(
                            target.join(family).join(format!("one.{ext}")),
                            if fixture == "zero" {
                                b"".as_slice()
                            } else {
                                b"fixture"
                            },
                        )
                        .unwrap();
                    }
                    if fixture == "corrupt" {
                        fs::create_dir_all(target.join(".jmcl")).unwrap();
                        fs::write(target.join(".jmcl/content.json"), b"{").unwrap();
                    }
                    if fixture == "directory" {
                        fs::create_dir(target.join(family).join(format!("one.{ext}"))).unwrap();
                    }
                    #[cfg(unix)]
                    if fixture == "alias" {
                        let real = t.path().join("outside");
                        fs::create_dir(&real).unwrap();
                        fs::write(real.join(format!("one.{ext}")), b"fixture").unwrap();
                        fs::remove_dir(target.join(family)).unwrap();
                        std::os::unix::fs::symlink(&real, target.join(family)).unwrap();
                    }
                    let r = run(
                        exe,
                        &["rpc"],
                        Some(
                            &json!({"protocol":1,"id":"adopt","method":format!("{family}.adopt"),"params":params}),
                        ),
                        t.path(),
                    );
                    let disk = files(&game);
                    results.push((r, disk));
                }
                assert_eq!(
                    results[0], results[1],
                    "family={family} fixture={fixture} params={params}"
                );
                count += 1;
            }
        }
    }
    eprintln!("{count} content adoption raw RPC and disk fixtures matched");
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE"]
fn adopt_cli_matches_oracle() {
    let zig = std::path::PathBuf::from(std::env::var_os("JMCL_ZIG_ORACLE").unwrap());
    let rust = Path::new(env!("CARGO_BIN_EXE_jmcl-core-rust"));
    let mut cases = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        let dp = family == "datapacks";
        let valid = if dp {
            vec!["demo", "--world", "World", "--store-directory", "cache"]
        } else {
            vec!["demo", "--store-directory", "cache"]
        };
        for args in [
            vec![],
            vec!["bad/id"],
            vec!["demo"],
            vec!["demo", "--bad", "x"],
            vec!["demo", "--store-directory"],
            vec!["demo", "--store-directory", ""],
            valid.clone(),
        ] {
            let mut results = vec![];
            for exe in [zig.as_path(), rust] {
                let t = tempfile::tempdir().unwrap();
                let created = run(
                    &zig,
                    &[
                        "instance",
                        "create",
                        "demo",
                        "--version",
                        "1.20.1",
                        "--no-validate",
                    ],
                    None,
                    t.path(),
                );
                assert_eq!(created.0, 0, "create fixture {:?}", created);
                let game = t.path().join("instances/demo/.minecraft");
                let target = if dp {
                    game.join("saves/World/datapacks")
                } else {
                    game.join(family)
                };
                fs::create_dir_all(&target).unwrap();
                fs::write(
                    target.join(if family == "mods" {
                        "one.jar"
                    } else {
                        "one.zip"
                    }),
                    b"fixture",
                )
                .unwrap();
                let mut full = vec!["instance", family, "adopt"];
                full.extend(&args);
                results.push((run(exe, &full, None, t.path()), files(&game)));
            }
            assert_eq!(results[0], results[1], "{family} {args:?}");
            if args == valid {
                assert_eq!(results[0].0.0, 0);
                assert!(String::from_utf8_lossy(&results[0].0.1).contains("adoptedCount\t1\n"));
            }
            cases += 1;
        }
    }
    eprintln!("{cases} raw adoption CLI and disk fixtures matched");
}

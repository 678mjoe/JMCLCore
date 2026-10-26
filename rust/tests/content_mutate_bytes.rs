//! Only disposable opaque local entries; selectors remain ordinary ASCII.
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
#[cfg(unix)]
fn raw_name(bytes: &[u8]) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes).to_owned()
}
#[cfg(windows)]
fn raw_name(bytes: &[u8]) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    // Test corpus uses exactly one unpaired high surrogate followed by ASCII.
    assert!(bytes.starts_with(b"\xed\xa0\x80"));
    std::ffi::OsString::from_wide(
        &std::iter::once(0xd800)
            .chain(bytes[3..].iter().map(|b| u16::from(*b)))
            .collect::<Vec<_>>(),
    )
}
fn identity(path: &Path) -> (u64, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(path).unwrap();
        (m.dev(), m.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let f = fs::File::open(path).unwrap();
        let mut i = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        assert_ne!(
            unsafe { GetFileInformationByHandle(f.as_raw_handle(), i.as_mut_ptr()) },
            0
        );
        let i = unsafe { i.assume_init() };
        (
            u64::from(i.dwVolumeSerialNumber),
            (u64::from(i.nFileIndexHigh) << 32) | u64::from(i.nFileIndexLow),
        )
    }
}
fn physical_name_unsupported(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error())
    }
    #[cfg(windows)]
    {
        let _ = error;
        false
    }
}

fn pinned_oracle() -> String {
    let source = std::env::var("JMCL_ZIG_SOURCE").expect("pinned source checkout required");
    let head = Command::new("git")
        .args(["-C", &source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .args(["-C", &source, "diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    std::env::var("JMCL_ZIG_ORACLE").unwrap()
}

fn run(exe: &str, root: &Path, req: &Value, args: &[String], cli: bool) -> (i32, Vec<u8>, Vec<u8>) {
    let rpc = ["rpc".into()];
    let mut c = Command::new(exe)
        .args(if cli { args } else { &rpc })
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if !cli {
        writeln!(c.stdin.take().unwrap(), "{req}").unwrap();
    }
    drop(c.stdin.take());
    let o = c.wait_with_output().unwrap();
    (o.status.code().unwrap(), o.stdout, o.stderr)
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and JMCL_ZIG_ORACLE"]
fn raw_entry_toggle_remove_preserve_immutable_shared_objects() {
    let zig = pinned_oracle();
    for server in [false, true] {
        if server && !cfg!(target_os = "linux") {
            continue;
        }
        for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
            if server && !matches!(family, "mods" | "datapacks") {
                continue;
            }
            for physical in ["utf8", "wtf8", "invalid_utf8"] {
                if physical == "invalid_utf8" && cfg!(windows) {
                    continue;
                }
                for state in ["present", "missing_file", "missing_parent"] {
                    for remove_enabled in [false, true] {
                        for array_selector in [false, true] {
                            for cli in [false, true] {
                                if cli && array_selector {
                                    continue;
                                }
                                let mut results = vec![];
                                for exe in [zig.as_str(), env!("CARGO_BIN_EXE_jmcl-core-rust")] {
                                    let t = tempfile::tempdir().unwrap();
                                    let home = t.path().join("demo");
                                    fs::create_dir(&home).unwrap();
                                    let game = if server {
                                        fs::write(
                                            home.join("server.json"),
                                            br#"{"id":"demo","name":"Demo","version_id":"demo"}"#,
                                        )
                                        .unwrap();
                                        fs::write(
                                            home.join("server.properties"),
                                            b"level-name=World\n",
                                        )
                                        .unwrap();
                                        home.clone()
                                    } else {
                                        fs::write(home.join("instance.json"),br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"demo","source":"official"}"#).unwrap();
                                        home.join(".minecraft")
                                    };
                                    let root = if family == "datapacks" {
                                        game.join(if server { "World" } else { "saves/World" })
                                    } else {
                                        game
                                    };
                                    fs::create_dir_all(root.join(family)).unwrap();
                                    fs::create_dir(root.join(".jmcl")).unwrap();
                                    let ext = if family == "mods" { "jar" } else { "zip" };
                                    let kind = match family {
                                        "mods" => "mod",
                                        "resourcepacks" => "resourcepack",
                                        "shaderpacks" => "shaderpack",
                                        _ => "datapack",
                                    };
                                    let name = match physical {
                                        "utf8" => format!("ordinary.{ext}").into_bytes(),
                                        "wtf8" => {
                                            [b"\xed\xa0\x80.".as_slice(), ext.as_bytes()].concat()
                                        }
                                        _ => [b"\xff.".as_slice(), ext.as_bytes()].concat(),
                                    };
                                    let native = if physical == "utf8" {
                                        std::ffi::OsString::from(
                                            std::str::from_utf8(&name).unwrap(),
                                        )
                                    } else {
                                        raw_name(&name)
                                    };
                                    let path = root.join(family).join(&native);
                                    let mut disabled_native = native.clone();
                                    disabled_native.push(".disabled");
                                    let disabled = root.join(family).join(disabled_native);
                                    if let Err(e) = fs::write(&path, b"shared opaque payload") {
                                        if physical != "utf8" && physical_name_unsupported(&e) {
                                            assert!(!path.exists());
                                            eprintln!("physical {physical} excluded by {e}");
                                            break;
                                        }
                                        panic!("{e}");
                                    }
                                    let peer = t.path().join("peer");
                                    fs::hard_link(&path, &peer).unwrap();
                                    let hash =
                                        format!("{:x}", Sha1::digest(b"shared opaque payload"));
                                    let object = t
                                        .path()
                                        .join("cache")
                                        .join(jmcl_core_rust::store::object_path(&hash).unwrap());
                                    fs::create_dir_all(object.parent().unwrap()).unwrap();
                                    fs::hard_link(&path, &object).unwrap();
                                    if state != "present" {
                                        fs::remove_file(&path).unwrap();
                                    }
                                    if state == "missing_parent" {
                                        fs::remove_dir(root.join(family)).unwrap();
                                    }
                                    let mut permissions =
                                        fs::metadata(&object).unwrap().permissions();
                                    permissions.set_readonly(true);
                                    fs::set_permissions(&object, permissions).unwrap();
                                    let id = identity(&object);
                                    let file = [family.as_bytes(), b"/", &name].concat();
                                    let file = match std::str::from_utf8(&file) {
                                        Ok(s) => json!(s),
                                        Err(_) => json!(file),
                                    };
                                    let retained = json!({"kind":kind,"file":([family.as_bytes(),b"/retained-\xff.",ext.as_bytes()].concat()),"sha1":"b".repeat(40),"size":1,"source":{"provider":"manual","project_id":null,"version_id":null,"slug":null},"enabled":false,"as_dependency":false});
                                    let entry = json!({"kind":kind,"file":file,"sha1":hash,"size":21,"source":{"provider":"modrinth","project_id":"project","version_id":"version","slug":"slug"},"enabled":true,"as_dependency":false});
                                    let registry = root.join(".jmcl/content.json");
                                    fs::write(
                                &registry,
                                serde_json::to_vec(
                                    &json!({"schema_version":1,"entries":[entry,retained.clone()]}),
                                )
                                .unwrap(),
                            )
                            .unwrap();
                                    let mut steps = vec![];
                                    for op in ["disable", "disable", "enable", "remove", "remove"] {
                                        if op == "enable" && !remove_enabled {
                                            continue;
                                        }
                                        let mut params = if server {
                                            json!({"directory":".","id":"demo","project":"project"})
                                        } else {
                                            json!({"directory":"demo/.minecraft","world":"World","project":"project"})
                                        };
                                        if array_selector {
                                            params.as_object_mut().unwrap().remove("project");
                                            params["file"] = json!(
                                                [family.as_bytes(), b"/", name.as_slice()].concat()
                                            );
                                        }
                                        let req = json!({"protocol":1,"id":"mutate","method":format!("{}{family}.{op}",if server{"server."}else{""}),"params":params});
                                        let mut args = vec![
                                            if server { "server" } else { "instance" }.into(),
                                            family.into(),
                                            op.into(),
                                            "demo".into(),
                                            if server {
                                                "--servers-directory"
                                            } else {
                                                "--instances-directory"
                                            }
                                            .into(),
                                            ".".into(),
                                            "--project".into(),
                                            "project".into(),
                                        ];
                                        if !server && family == "datapacks" {
                                            args.extend(["--world".into(), "World".into()]);
                                        }
                                        let output = run(exe, t.path(), &req, &args, cli);
                                        let disk = fs::read(&registry).unwrap();
                                        let value: Value = serde_json::from_slice(&disk).unwrap();
                                        assert_eq!(
                                            value["entries"].as_array().unwrap().last().unwrap(),
                                            &retained
                                        );
                                        if state != "present" {
                                            assert!(!path.exists());
                                            assert!(!disabled.exists());
                                            if state == "missing_parent" {
                                                assert!(!root.join(family).exists());
                                            }
                                        } else if op == "disable" {
                                            assert!(!path.exists());
                                            assert_eq!(identity(&disabled), id);
                                        } else if op == "enable" {
                                            assert!(!disabled.exists());
                                            assert_eq!(identity(&path), id);
                                        } else {
                                            assert!(!path.exists());
                                            assert!(!disabled.exists());
                                        }
                                        for p in [&peer, &object] {
                                            assert_eq!(identity(p), id);
                                            assert_eq!(
                                                fs::read(p).unwrap(),
                                                b"shared opaque payload"
                                            );
                                            assert!(
                                                fs::metadata(p).unwrap().permissions().readonly()
                                            );
                                        }
                                        steps.push((output, disk));
                                    }
                                    results.push(steps);
                                }
                                if results.len() == 2 {
                                    assert_eq!(
                                        results[0], results[1],
                                        "{family} server={server} physical={physical} cli={cli} state={state} remove_enabled={remove_enabled} array_selector={array_selector}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and JMCL_ZIG_ORACLE"]
fn selector_errors_precede_registry_parse_without_disk_changes() {
    let zig = pinned_oracle();
    for op in ["disable", "enable", "remove"] {
        for selector in [
            json!({"project":"../bad"}),
            json!({"file":"../bad"}),
            json!({"project":"project","file":"mods/a.jar"}),
        ] {
            let mut outputs = vec![];
            for exe in [zig.as_str(), env!("CARGO_BIN_EXE_jmcl-core-rust")] {
                let t = tempfile::tempdir().unwrap();
                let game = t.path().join("game");
                fs::create_dir_all(game.join(".jmcl")).unwrap();
                let registry = game.join(".jmcl/content.json");
                fs::write(&registry, b"{invalid registry").unwrap();
                let before = identity(&registry);
                let mut params = selector.clone();
                params["directory"] = json!("game");
                let req = json!({"protocol":1,"id":"invalid","method":format!("mods.{op}"),"params":params});
                let output = run(exe, t.path(), &req, &[], false);
                assert_eq!(
                    serde_json::from_slice::<Value>(&output.1).unwrap()["error"]["code"],
                    "INVALID_PARAMS"
                );
                assert_eq!(fs::read(&registry).unwrap(), b"{invalid registry");
                assert_eq!(identity(&registry), before);
                outputs.push(output);
            }
            assert_eq!(outputs[0], outputs[1]);
        }
    }
}
#[test]
fn public_string_mutation_apis_still_reject_raw_registry_without_changes() {
    use jmcl_core_rust::content::{Root, registry::Kind};
    let t = tempfile::tempdir().unwrap();
    let root = Root::open(t.path().to_str().unwrap()).unwrap();
    fs::create_dir(t.path().join(".jmcl")).unwrap();
    let path = t.path().join(".jmcl/content.json");
    let bytes=serde_json::to_vec(&json!({"schema_version":1,"entries":[{"kind":"mod","file":b"mods/raw-\xff.jar","sha1":"a".repeat(40),"size":1,"source":{"provider":"modrinth","project_id":"project","version_id":"version"}}]})).unwrap();
    fs::write(&path, &bytes).unwrap();
    let before = identity(&path);
    assert!(
        root.toggle(Kind::Mod, Some("project"), None, false)
            .is_err()
    );
    assert!(root.remove(Kind::Mod, Some("project"), None).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(identity(&path), before);
}

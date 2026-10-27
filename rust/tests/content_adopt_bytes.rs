//! Disposable opaque files only: no archive parsing, acquisition or workloads.
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn run(exe: &str, root: &Path, request: &Value, cli: Option<&[String]>) -> Vec<u8> {
    let mut child = Command::new(exe)
        .args(cli.unwrap_or(&["rpc".into()]))
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if cli.is_none() {
        writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    out.stdout
}
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
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE"]
fn opaque_raw_adoption_matches_pinned_oracle() {
    let zig = pinned_oracle();
    for server in [false, true] {
        if server && !cfg!(target_os = "linux") {
            continue;
        }
        for surface in ["rpc", "cli"] {
            for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
                if server && !matches!(family, "mods" | "datapacks") {
                    continue;
                }
                for physical in ["utf8", "wtf8", "invalid_utf8"] {
                    if physical == "invalid_utf8" && cfg!(windows) {
                        continue;
                    }
                    let physical_raw = physical != "utf8";
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
                            fs::write(home.join("server.properties"), b"level-name=World\n")
                                .unwrap();
                            home.clone()
                        } else {
                            fs::write(home.join("instance.json"),br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"demo","source":"official"}"#).unwrap();
                            home.join(".minecraft")
                        };
                        let content = if family == "datapacks" {
                            game.join(if server { "World" } else { "saves/World" })
                        } else {
                            game.clone()
                        };
                        fs::create_dir_all(content.join(family)).unwrap();
                        fs::create_dir_all(content.join(".jmcl")).unwrap();
                        let ext = if family == "mods" { "jar" } else { "zip" };
                        let kind = match family {
                            "mods" => "mod",
                            "resourcepacks" => "resourcepack",
                            "shaderpacks" => "shaderpack",
                            _ => "datapack",
                        };
                        let retained =
                            [family.as_bytes(), b"/retained-\xff.", ext.as_bytes()].concat();
                        let entry = json!({"kind":kind,"file":retained,"sha1":"a".repeat(40),"size":1,"source":{"provider":"manual","project_id":null,"version_id":null,"slug":null},"enabled":false,"as_dependency":false});
                        fs::write(
                            content.join(".jmcl/content.json"),
                            serde_json::to_vec(
                                &json!({"schema_version":1,"entries":[entry.clone()]}),
                            )
                            .unwrap(),
                        )
                        .unwrap();
                        let name = if physical_raw {
                            raw_name(
                                &[
                                    if physical == "wtf8" {
                                        b"\xed\xa0\x80.".as_slice()
                                    } else {
                                        b"\xff.".as_slice()
                                    },
                                    ext.as_bytes(),
                                ]
                                .concat(),
                            )
                        } else {
                            format!("ordinary.{ext}").into()
                        };
                        let path = content.join(family).join(name);
                        if let Err(e) = fs::write(&path, b"opaque payload, never parsed") {
                            if physical_raw && physical_name_unsupported(&e) {
                                assert!(!path.exists());
                                assert_eq!(
                                    serde_json::from_slice::<Value>(
                                        &fs::read(content.join(".jmcl/content.json")).unwrap()
                                    )
                                    .unwrap()["entries"][0],
                                    entry
                                );
                                eprintln!(
                                    "physical raw name unsupported: {e}; registry retention remains mandatory"
                                );
                                break;
                            }
                            panic!("{e}")
                        }
                        let id = identity(&path);
                        fs::hard_link(&path, t.path().join("peer")).unwrap();
                        let params = if server {
                            json!({"directory":".","id":"demo","store_directory":"cache"})
                        } else {
                            json!({"directory":"demo/.minecraft","world":"World","store_directory":"cache"})
                        };
                        let req = json!({"protocol":1,"id":"bytes","method":format!("{}{family}.adopt",if server {"server."}else{""}),"params":params});
                        let mut cli = vec![
                            if server { "server" } else { "instance" }.into(),
                            family.into(),
                            "adopt".into(),
                            "demo".into(),
                            if server {
                                "--servers-directory"
                            } else {
                                "--instances-directory"
                            }
                            .into(),
                            ".".into(),
                            "--store-directory".into(),
                            "cache".into(),
                        ];
                        if !server && family == "datapacks" {
                            cli.extend(["--world".into(), "World".into()]);
                        }
                        let cli = if surface == "cli" {
                            Some(cli.as_slice())
                        } else {
                            None
                        };
                        let first = run(exe, t.path(), &req, cli);
                        if surface == "rpc" {
                            let parsed: Value = serde_json::from_slice(&first).unwrap();
                            assert!(parsed.get("error").is_none(), "{exe}: {parsed}");
                            assert_eq!(parsed["result"]["adopted"].as_array().unwrap().len(), 1);
                        }
                        let disk = fs::read(content.join(".jmcl/content.json")).unwrap();
                        let registry: Value = serde_json::from_slice(&disk).unwrap();
                        assert_eq!(registry["entries"][0], entry);
                        let adopted = registry["entries"].as_array().unwrap();
                        assert_eq!(adopted.len(), 2);
                        let hash = adopted[1]["sha1"].as_str().unwrap();
                        assert_eq!(
                            hash,
                            format!("{:x}", Sha1::digest(b"opaque payload, never parsed"))
                        );
                        let object = t
                            .path()
                            .join("cache")
                            .join(jmcl_core_rust::store::object_path(hash).unwrap());
                        assert_eq!(identity(&object), id);
                        assert_eq!(identity(&path), id);
                        assert_eq!(identity(&t.path().join("peer")), id);
                        assert_eq!(fs::read(&object).unwrap(), b"opaque payload, never parsed");
                        assert!(fs::metadata(&object).unwrap().permissions().readonly());
                        let second = run(exe, t.path(), &req, cli);
                        if surface == "rpc" {
                            assert_eq!(
                                serde_json::from_slice::<Value>(&second).unwrap()["result"]["adopted"],
                                json!([])
                            );
                        } else {
                            assert_eq!(second, b"adoptedCount\t0\n");
                        }
                        assert_eq!(fs::read(content.join(".jmcl/content.json")).unwrap(), disk);
                        results.push((first, second, disk));
                    }
                    if results.len() == 2 {
                        assert_eq!(
                            results[0], results[1],
                            "{family} physical={physical} server={server} surface={surface}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE"]
fn empty_opaque_adoption_preserves_registry_and_peer() {
    let zig = pinned_oracle();
    let mut results = vec![];
    for exe in [zig.as_str(), env!("CARGO_BIN_EXE_jmcl-core-rust")] {
        let t = tempfile::tempdir().unwrap();
        let game = t.path().join("game");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::create_dir(game.join(".jmcl")).unwrap();
        let registry = serde_json::to_vec(&json!({"schema_version":1,"entries":[{"kind":"mod","file":b"mods/retained-\xff.jar","sha1":"a".repeat(40),"size":1,"source":{"provider":"manual"}}]})).unwrap();
        fs::write(game.join(".jmcl/content.json"), &registry).unwrap();
        let file = game.join("mods/empty.jar");
        fs::write(&file, b"").unwrap();
        fs::hard_link(&file, t.path().join("peer")).unwrap();
        let before = identity(&file);
        let req = json!({"protocol":1,"id":"zero","method":"mods.adopt","params":{"directory":"game","store_directory":"cache"}});
        let output = run(exe, t.path(), &req, None);
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap()["error"]["code"],
            "CONTENT_ADOPT_FAILED"
        );
        assert_eq!(fs::read(game.join(".jmcl/content.json")).unwrap(), registry);
        assert_eq!(identity(&file), before);
        assert_eq!(identity(&t.path().join("peer")), before);
        assert!(!fs::metadata(&file).unwrap().permissions().readonly());
        let objects = t.path().join("cache/objects/sha1");
        assert!(!objects.exists() || fs::read_dir(objects).unwrap().next().is_none());
        results.push(output);
    }
    assert_eq!(results[0], results[1]);
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

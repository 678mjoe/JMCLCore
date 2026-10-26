//! Read/list only: raw paths exist only in synthetic registry records.
//! No adoption, mutations, archive parsing, downloads or launched workloads.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn run(binary: &str, args: &[String], input: Option<&[u8]>) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap(), out.stdout, out.stderr)
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
type DiskEntry = (PathBuf, Vec<u8>, bool, (u64, u64));
fn snapshot(root: &Path) -> Vec<DiskEntry> {
    let mut result = vec![];
    fn visit(root: &Path, dir: &Path, out: &mut Vec<DiskEntry>) {
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            let m = fs::symlink_metadata(&path).unwrap();
            if m.is_dir() {
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    vec![],
                    m.permissions().readonly(),
                    (0, 0),
                ));
                visit(root, &path, out);
            } else {
                let identity = if m.is_symlink() {
                    (0, 0)
                } else {
                    identity(&path)
                };
                let bytes = if m.is_symlink() {
                    vec![]
                } else {
                    fs::read(&path).unwrap()
                };
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    bytes,
                    m.permissions().readonly(),
                    identity,
                ));
            }
        }
    }
    visit(root, root, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
fn fixture_files(dir: &Path, ext: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("a.{ext}")), b"opaque managed bytes").unwrap();
    fs::write(
        dir.join(format!("unmanaged.{ext}")),
        b"opaque unmanaged bytes",
    )
    .unwrap();
    let path = dir.join(format!("a.{ext}"));
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms).unwrap();
}
fn registry(family: &str, case: &str) -> Value {
    let kind = match family {
        "mods" => "mod",
        "resourcepacks" => "resourcepack",
        "shaderpacks" => "shaderpack",
        _ => "datapack",
    };
    let ext = if family == "mods" { "jar" } else { "zip" };
    let entry = |file: Value| json!({"kind":kind,"file":file,"sha1":"a".repeat(40),"size":1,"source":{"provider":"manual","project_id":null,"version_id":null,"slug":null},"enabled":true,"as_dependency":false});
    // Deliberately do NOT create these raw paths on disk. Registry listing
    // preserves source records even when a native filesystem cannot name them.
    let raw = [family.as_bytes(), b"/raw-\xff.", ext.as_bytes()].concat();
    let surrogate = [
        family.as_bytes(),
        b"/surrogate-\xed\xa0\x80.",
        ext.as_bytes(),
    ]
    .concat();
    let mut value = json!({"schema_version":1,"entries":[entry(json!(format!("{family}/a.{ext}"))),entry(json!(raw)),entry(json!(surrogate)),entry(json!(format!("{family}/é.{ext}")))]});
    match case {
        "duplicate" => {
            let mut same = value["entries"][0].clone();
            same["file"] = json!(format!("{family}/a.{ext}").as_bytes());
            value["entries"].as_array_mut().unwrap().push(same);
        }
        "unsafe" => {
            value["entries"][1]["file"] =
                json!([family.as_bytes(), b"/../raw-\xff.", ext.as_bytes()].concat())
        }
        "prefix" => {
            value["entries"][1]["file"] =
                json!([b"wrong/raw-\xff.".as_slice(), ext.as_bytes()].concat())
        }
        "hash" => value["entries"][0]["sha1"] = json!("invalid"),
        "long" => {
            value["entries"][1]["file"] =
                json!([family.as_bytes(), b"/", vec![b'x'; 512].as_slice()].concat())
        }
        "unsafe_utf8" => value["entries"] = json!([entry(json!(format!("{family}/../bad.{ext}")))]),
        "source_mr" | "source_cf" => {
            value["entries"] = json!([entry(json!(format!("{family}/a.{ext}")))]);
            value["entries"][0]["source"] = json!({"provider":if case=="source_mr"{"modrinth"}else{"curseforge"},"project_id":"bad/ref","version_id":"12","slug":null});
        }
        "success" => (),
        _ => panic!("bounded fixture"),
    }
    value
}
fn check_family(server: bool, family: &str, case: &str) {
    let zig = std::env::var("JMCL_ZIG_ORACLE").expect("pinned oracle required");
    let rust = env!("CARGO_BIN_EXE_jmcl-core-rust");
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    let home = base.join("demo");
    fs::create_dir_all(&home).unwrap();
    let game = if server {
        fs::write(
            home.join("server.json"),
            br#"{"id":"demo","name":"Demo","version_id":"demo"}"#,
        )
        .unwrap();
        fs::write(home.join("server.properties"), b"level-name=World\n").unwrap();
        home.clone()
    } else {
        fs::write(home.join("instance.json"), br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"demo","source":"official"}"#).unwrap();
        home.join(".minecraft")
    };
    let content = if family == "datapacks" {
        game.join(if server { "World" } else { "saves/World" })
    } else {
        game.clone()
    };
    fixture_files(
        &content.join(family),
        if family == "mods" { "jar" } else { "zip" },
    );
    fs::create_dir_all(content.join(".jmcl")).unwrap();
    let record = registry(family, if case == "alias" { "success" } else { case });
    fs::write(
        content.join(".jmcl/content.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    if case == "alias" {
        let external = base.join("outside-registry.json");
        fs::rename(content.join(".jmcl/content.json"), &external).unwrap();
        std::os::unix::fs::symlink(&external, content.join(".jmcl/content.json")).unwrap();
    }
    let original = content
        .join(family)
        .join(if family == "mods" { "a.jar" } else { "a.zip" });
    fs::hard_link(&original, base.join("shared-opaque-file")).unwrap();
    assert_eq!(
        identity(&original),
        identity(&base.join("shared-opaque-file"))
    );
    let before = snapshot(base);
    let mut params = if server {
        json!({"directory":base,"id":"demo"})
    } else {
        json!({"directory":game})
    };
    if !server && family == "datapacks" {
        params["world"] = json!("World");
    }
    let method = format!("{}{family}.list", if server { "server." } else { "" });
    let input = format!(
        "{}\n",
        json!({"protocol":1,"id":"list","method":method,"params":params})
    );
    let args = vec!["rpc".into()];
    let old = run(&zig, &args, Some(input.as_bytes()));
    let new = run(rust, &args, Some(input.as_bytes()));
    assert_eq!(new, old, "{method} {case} raw wire");
    assert_eq!(new.0, 0);
    let response: Value = serde_json::from_slice(&new.1).unwrap();
    if matches!(case, "success" | "alias") {
        assert_eq!(response["result"]["entries"], record["entries"]);
        assert_eq!(
            response["result"]["unmanaged"],
            json!([format!(
                "{family}/unmanaged.{}",
                if family == "mods" { "jar" } else { "zip" }
            )])
        );
    } else {
        assert_eq!(response["event"], "error");
    }
    let mut cli = vec![
        if server { "server" } else { "instance" }.into(),
        family.into(),
        "list".into(),
        "demo".into(),
        if server {
            "--servers-directory"
        } else {
            "--instances-directory"
        }
        .into(),
        base.to_str().unwrap().into(),
    ];
    if !server && family == "datapacks" {
        cli.extend(["--world".into(), "World".into()]);
    }
    for json in [false, true] {
        if json {
            cli.push("--json".into());
        }
        let old = run(&zig, &cli, None);
        let new = run(rust, &cli, None);
        assert_eq!(new, old, "{cli:?}");
        assert_eq!(
            new.0,
            if matches!(case, "success" | "alias") {
                0
            } else {
                1
            }
        );
    }
    assert_eq!(
        snapshot(base),
        before,
        "listing changed files/identity/attributes"
    );
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn client_registry_byte_listing_preserves_records_and_disk() {
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for case in [
            "success",
            "duplicate",
            "unsafe",
            "prefix",
            "hash",
            "long",
            "unsafe_utf8",
            "source_mr",
            "source_cf",
        ] {
            check_family(false, family, case);
        }
    }
}
#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires pinned Zig executable; server RPC is Linux-only"]
fn server_registry_byte_listing_preserves_records_and_disk() {
    for family in ["mods", "datapacks"] {
        for case in [
            "success",
            "duplicate",
            "unsafe",
            "prefix",
            "hash",
            "long",
            "unsafe_utf8",
            "source_mr",
            "source_cf",
        ] {
            check_family(true, family, case);
        }
    }
}

#[test]
fn raw_listing_view_does_not_widen_string_load_or_write() {
    use jmcl_core_rust::content::{
        Root,
        registry::{Kind, Registry},
    };
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".jmcl")).unwrap();
    fs::create_dir(dir.path().join("mods")).unwrap();
    let document = registry("mods", "success");
    fs::write(
        dir.path().join(".jmcl/content.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let before = snapshot(dir.path());
    let root = Root::open(dir.path().to_str().unwrap()).unwrap();
    let (entries, unmanaged) = root.list_paths(Kind::Mod).unwrap();
    assert_eq!(serde_json::to_value(entries).unwrap(), document["entries"]);
    assert!(unmanaged.is_empty());
    assert!(
        root.load().is_err(),
        "legacy String load must still reject raw registered paths"
    );
    assert!(
        root.list(Kind::Mod).is_err(),
        "adoption's legacy read view must not broaden"
    );
    // Compile the unchanged String-specialized save signature; never invoke it.
    let _save: fn(&Root, &Registry) -> Result<(), jmcl_core_rust::content::Error> = Root::save;
    assert_eq!(snapshot(dir.path()), before);
}

#[cfg(unix)]
#[test]
#[ignore = "requires pinned Zig executable; raw-name capability is independently reported"]
fn physical_raw_registered_file_is_not_reported_unmanaged() {
    use std::os::unix::ffi::OsStringExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".jmcl")).unwrap();
    fs::create_dir(root.join("mods")).unwrap();
    let path = root
        .join("mods")
        .join(std::ffi::OsString::from_vec(b"raw-\xff.jar".to_vec()));
    match fs::write(&path, b"opaque registered content") {
        Ok(()) => (),
        Err(e) if e.raw_os_error() == Some(libc::EILSEQ) => {
            eprintln!(
                "physical raw registered-name suppression unavailable: EILSEQ; record-only native cases remain mandatory"
            );
            return;
        }
        Err(e) => panic!("unexpected raw-name capability error: {e}"),
    }
    fs::write(
        root.join(".jmcl/content.json"),
        serde_json::to_vec(&registry("mods", "success")).unwrap(),
    )
    .unwrap();
    let before = snapshot(root);
    let input = format!(
        "{}\n",
        json!({"protocol":1,"id":"raw","method":"mods.list","params":{"directory":root}})
    );
    let args = vec!["rpc".into()];
    let old = run(
        &std::env::var("JMCL_ZIG_ORACLE").unwrap(),
        &args,
        Some(input.as_bytes()),
    );
    let new = run(
        env!("CARGO_BIN_EXE_jmcl-core-rust"),
        &args,
        Some(input.as_bytes()),
    );
    assert_eq!(new, old);
    let response: Value = serde_json::from_slice(&new.1).unwrap();
    assert_eq!(response["result"]["unmanaged"], json!([]));
    assert_eq!(snapshot(root), before);
}

#[cfg(unix)]
#[test]
#[ignore = "requires pinned Zig executable"]
fn configured_registry_file_alias_is_read_without_rewrite() {
    check_family(false, "mods", "alias");
}

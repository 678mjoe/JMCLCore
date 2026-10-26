//! Listing opaque files only: no archive parsing, adoption, downloads or processes.
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
fn fixture_files(dir: &Path, ext: &str) -> Vec<Vec<u8>> {
    fs::create_dir_all(dir).unwrap();
    let names = vec![
        format!("a.{ext}").into_bytes(),
        format!("世界-é.{ext}").into_bytes(),
    ];
    #[cfg(unix)]
    let names = {
        let mut names = names;
        use std::os::unix::ffi::OsStringExt;
        let raw = [
            b"raw-\xff-".as_slice(),
            ext.as_bytes(),
            b".",
            ext.as_bytes(),
        ]
        .concat();
        match fs::write(
            dir.join(std::ffi::OsString::from_vec(raw.clone())),
            b"opaque bytes",
        ) {
            Ok(()) => names.push(raw),
            Err(e) if e.raw_os_error() == Some(libc::EILSEQ) => eprintln!(
                "raw-name success unavailable: filesystem EILSEQ; Unicode controls still required"
            ),
            Err(e) => panic!("unexpected raw-name creation error: {e}"),
        }
        names
    };
    for name in &names {
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStringExt;
            dir.join(std::ffi::OsString::from_vec(name.clone()))
        };
        #[cfg(not(unix))]
        let path = dir.join(std::str::from_utf8(name).unwrap());
        fs::write(&path, b"opaque bytes").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(path, perms).unwrap();
    }
    fs::write(dir.join(format!("ignored.{ext}.disabled")), b"disabled").unwrap();
    fs::create_dir(dir.join(format!("directory.{ext}"))).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        names[0].iter().map(|b| *b as char).collect::<String>(),
        dir.join(format!("link.{ext}")),
    )
    .unwrap();
    names
}
fn check_family(server: bool, family: &str) {
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
    let names = fixture_files(
        &content.join(family),
        if family == "mods" { "jar" } else { "zip" },
    );
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
    assert_eq!(new, old, "{method} raw wire");
    assert_eq!(new.0, 0);
    let response: Value = serde_json::from_slice(&new.1).unwrap();
    let actual = response["result"]["unmanaged"]
        .as_array()
        .expect("successful list");
    assert_eq!(actual.len(), names.len());
    for name in names {
        let bytes = [family.as_bytes(), b"/", &name].concat();
        let expected = match String::from_utf8(bytes.clone()) {
            Ok(s) => json!(s),
            Err(_) => json!(bytes),
        };
        assert!(actual.contains(&expected), "missing {expected}");
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
        assert_eq!(new.0, 0);
    }
    assert_eq!(
        snapshot(base),
        before,
        "listing changed files/identity/attributes"
    );
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn client_listing_preserves_opaque_names_and_disk() {
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        check_family(false, family);
    }
}
#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires pinned Zig executable; server RPC is Linux-only"]
fn server_listing_preserves_opaque_names_and_disk() {
    for family in ["mods", "datapacks"] {
        check_family(true, family);
    }
}

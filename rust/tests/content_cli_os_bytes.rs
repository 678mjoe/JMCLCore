//! Native argument tests: no physical raw filenames are required here.
use std::{ffi::OsString, fs, path::Path, process::Command};
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

#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and JMCL_ZIG_ORACLE"]
fn raw_file_argument_precedence_and_no_match_match_oracle() {
    let zig = pinned_oracle();
    for server in [false, true] {
        if server && !cfg!(target_os = "linux") {
            continue;
        }
        for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
            if server && !matches!(family, "mods" | "datapacks") {
                continue;
            }
            for op in ["enable", "disable", "remove"] {
                for case in [
                    "normal",
                    "duplicate_raw_last",
                    "duplicate_utf8_last",
                    "duplicate_empty",
                    "missing_value",
                    "unknown",
                    "both",
                    "consumed_flag",
                ] {
                    let mut outputs = vec![];
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
                            game
                        };
                        fs::create_dir_all(content.join(".jmcl")).unwrap();
                        let registry = content.join(".jmcl/content.json");
                        let bytes = b"{\"schema_version\":1,\"entries\":[]}";
                        fs::write(&registry, bytes).unwrap();
                        let before = identity(&registry);
                        let mut raw = OsString::from(format!("{family}/"));
                        raw.push(raw_name(b"\xed\xa0\x80.jar"));
                        let mut args: Vec<OsString> = vec![
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
                        ];
                        if !server && family == "datapacks" {
                            args.extend(["--world".into(), "World".into()]);
                        }
                        if case == "duplicate_raw_last" {
                            args.extend(["--file".into(), "mods/ordinary.jar".into()]);
                        }
                        if case == "consumed_flag" {
                            args.push("--file".into());
                        }
                        args.extend(["--file".into(), raw]);
                        match case {
                            "duplicate_utf8_last" => {
                                args.extend(["--file".into(), "mods/ordinary.jar".into()])
                            }
                            "duplicate_empty" => args.extend(["--file".into(), "".into()]),
                            "missing_value" => args.push("--file".into()),
                            "unknown" => args.extend(["--unknown".into(), "value".into()]),
                            "both" => args.extend(["--project".into(), "project".into()]),
                            _ => (),
                        }
                        let o = Command::new(exe)
                            .args(args)
                            .current_dir(t.path())
                            .output()
                            .unwrap();
                        assert_ne!(o.status.code(), Some(101));
                        assert_eq!(fs::read(&registry).unwrap(), bytes);
                        assert_eq!(identity(&registry), before);
                        outputs.push((o.status.code(), o.stdout, o.stderr));
                    }
                    assert_eq!(
                        outputs[0], outputs[1],
                        "server={server} {family}.{op} {case}"
                    );
                }
            }
        }
    }
}

#[test]
fn interception_does_not_claim_other_raw_routes_or_fields() {
    use jmcl_core_rust::content::local_cli_os;
    let raw = raw_name(b"\xed\xa0\x80.jar");
    for mut args in [
        vec![
            "instance".into(),
            "worlds".into(),
            "remove".into(),
            "demo".into(),
            "--file".into(),
        ],
        vec![
            "instance".into(),
            "mods".into(),
            "list".into(),
            "demo".into(),
            "--file".into(),
        ],
        vec![
            "__server_supervisor".into(),
            "mods".into(),
            "remove".into(),
            "demo".into(),
            "--file".into(),
        ],
        vec![
            "instance".into(),
            "mods".into(),
            "remove".into(),
            "demo".into(),
            "--instances-directory".into(),
        ],
        vec![
            "instance".into(),
            "mods".into(),
            "remove".into(),
            "demo".into(),
            "--project".into(),
        ],
    ] {
        args.push(raw.clone());
        let mut out = vec![];
        let mut err = vec![];
        assert!(local_cli_os(&args, &mut out, &mut err).is_none());
        assert!(out.is_empty());
        assert!(err.is_empty());
    }
    let args: Vec<OsString> = [
        "instance",
        "mods",
        "remove",
        "demo",
        "--file",
        "mods/ascii.jar",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    assert!(local_cli_os(&args, &mut vec![], &mut vec![]).is_none());
}

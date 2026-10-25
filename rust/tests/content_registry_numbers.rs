//! Bounded typed registry grammar and mutation-preservation oracle corpus.
//! Synthetic local files only; no providers, credentials, Java or network.
use jmcl_core_rust::content;
use serde_json::{json, value::RawValue};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

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
fn fixture(root: &Path, field: &str, token: &str) -> (std::path::PathBuf, Vec<u8>) {
    let game = root.join("demo/.minecraft");
    fs::create_dir_all(game.join(".jmcl")).unwrap();
    fs::create_dir_all(game.join("mods")).unwrap();
    fs::write(root.join("demo/instance.json"), br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"fixture","source":"official"}"#).unwrap();
    fs::write(root.join("shared.jar"), b"abc").unwrap();
    fs::hard_link(root.join("shared.jar"), game.join("mods/a.jar")).unwrap();
    let schema = if field == "schema_version" {
        token
    } else {
        "1"
    };
    let size = if field == "size" { token } else { "3" };
    let raw = format!(r#"{{"schema_version":{schema},"ignored":{{"nested":"ignored"}},"entries":[{{"kind":"mod","file":"mods/a.jar","sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":{size},"source":{{"provider":"manual","ignored":17}},"ignored":false}}]}}"#).into_bytes();
    fs::write(game.join(".jmcl/content.json"), &raw).unwrap();
    (game, raw)
}
fn rpc(binary: Option<&str>, game: &Path, op: &str) -> Vec<u8> {
    let method = format!("mods.{op}");
    let mut params = json!({"directory":game});
    if op != "list" {
        params["file"] = json!("mods/a.jar");
    }
    if let Some(binary) = binary {
        let request = json!({"protocol":1,"id":"n","method":method,"params":params});
        let mut child = Command::new(binary)
            .arg("rpc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        output.stdout
    } else {
        let raw = RawValue::from_string(params.to_string()).unwrap();
        let response = match content::handle(&method, Some(&raw)).unwrap() {
            Ok(value) => json!({"protocol":1,"id":"n","event":"result","result":value}),
            Err(error) => {
                json!({"protocol":1,"id":"n","event":"error","error":{"code":error.code,"message":error.message}})
            }
        };
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        bytes
    }
}
fn cli(binary: Option<&str>, root: &Path, op: &str) -> (i32, Vec<u8>, Vec<u8>) {
    let mut args = vec![
        "mods".to_owned(),
        op.to_owned(),
        "demo".to_owned(),
        "--instances-directory".to_owned(),
        root.to_str().unwrap().to_owned(),
    ];
    if op == "list" {
        args.push("--json".to_owned());
    } else {
        args.extend(["--file".to_owned(), "mods/a.jar".to_owned()]);
    }
    if let Some(binary) = binary {
        let output = Command::new(binary)
            .arg("instance")
            .args(args)
            .output()
            .unwrap();
        (output.status.code().unwrap(), output.stdout, output.stderr)
    } else {
        let (mut out, mut err) = (vec![], vec![]);
        let code = content::cli(&args, &mut out, &mut err).unwrap().unwrap();
        (code.into(), out, err)
    }
}
fn cases(field: &str) -> Vec<(&'static str, bool)> {
    let mut values = vec![
        ("1", true),
        ("1.0", true),
        (r#""1""#, true),
        (r#""0_1""#, true),
        (r#""1_0e-1""#, true),
        (r#""0x1.0p0""#, true),
        ("1.0000000000000000000000000000000000000001", true),
        ("1.0000000000000001", false),
        (r#""1.0000000000000001""#, false),
        ("0.99999999999999999", false),
        ("null", false),
        ("true", false),
        ("[]", false),
        ("{}", false),
        ("-1", false),
        (r#""1_""#, false),
        (r#"" 1""#, false),
        ("1e9999", false),
        ("4294967297", false),
        (r#""18446744073709551615.0""#, false),
        ("18446744073709551616", false),
    ];
    if field == "size" {
        values.extend([
            ("4294967296", true),
            (r#""4_294_967_296""#, true),
            ("4294967296.0000001", false),
            ("0", false),
        ]);
    } else {
        values.extend([
            ("4294967296", false),
            (r#""4_294_967_296""#, false),
            ("2", false),
            ("0", false),
        ]);
    }
    values
}
#[test]
#[ignore = "requires pinned Zig executable"]
fn typed_registry_numbers_match_wire_cli_and_preserve_files() {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").expect("pinned oracle required");
    let mut count = 0;
    for field in ["schema_version", "size"] {
        for (token, valid) in cases(field) {
            for op in ["list", "enable", "disable", "remove"] {
                for use_cli in [false, true] {
                    let mut outputs = vec![];
                    let mut disks = vec![];
                    for binary in [Some(oracle.as_str()), None] {
                        let temp = tempfile::tempdir().unwrap();
                        let root = temp.path();
                        let (game, original) = fixture(root, field, token);
                        let sibling = root.join("shared.jar");
                        let before = identity(&sibling);
                        assert_eq!(identity(&game.join("mods/a.jar")), before);
                        // Invalid registries must not even alter shared inode attributes.
                        if !valid {
                            let mut perms = fs::metadata(&sibling).unwrap().permissions();
                            perms.set_readonly(true);
                            fs::set_permissions(&sibling, perms).unwrap();
                        }
                        let output = if use_cli {
                            cli(binary, root, op)
                        } else {
                            (0, rpc(binary, &game, op), vec![])
                        };
                        if use_cli {
                            assert_eq!(output.0 == 0, valid, "{field} {token} {op}: {output:?}");
                        } else {
                            let v: serde_json::Value = serde_json::from_slice(&output.1).unwrap();
                            assert_eq!(
                                v.get("result").is_some(),
                                valid,
                                "{field} {token} {op}: {v}"
                            );
                        }
                        assert_eq!(fs::read(&sibling).unwrap(), b"abc");
                        assert_eq!(identity(&sibling), before);
                        assert_eq!(
                            fs::metadata(&sibling).unwrap().permissions().readonly(),
                            !valid
                        );
                        let registry = fs::read(game.join(".jmcl/content.json")).unwrap();
                        let mut names: Vec<_> = fs::read_dir(game.join("mods"))
                            .unwrap()
                            .map(|e| e.unwrap().file_name())
                            .collect();
                        names.sort();
                        if !valid || op == "list" || op == "enable" {
                            assert_eq!(registry, original);
                            assert_eq!(names, [std::ffi::OsString::from("a.jar")]);
                            assert_eq!(identity(&game.join("mods/a.jar")), before);
                        } else if op == "disable" {
                            assert_eq!(names, [std::ffi::OsString::from("a.jar.disabled")]);
                            assert_eq!(identity(&game.join("mods/a.jar.disabled")), before);
                        } else {
                            assert!(names.is_empty());
                        }
                        outputs.push(output);
                        disks.push((registry, names));
                    }
                    assert_eq!(outputs[0], outputs[1], "{field} {token} {op} cli={use_cli}");
                    assert_eq!(disks[0], disks[1], "{field} {token} {op} cli={use_cli}");
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 400);
}

#[test]
fn typed_registry_numeric_rejection_and_legacy_grammar() {
    for field in ["schema_version", "size"] {
        for (token, valid) in cases(field) {
            let t = tempfile::tempdir().unwrap();
            let (_, raw) = fixture(t.path(), field, token);
            assert_eq!(
                content::registry::parse(&raw).is_ok(),
                valid,
                "{field} {token}"
            );
        }
    }
    assert_eq!(
        content::registry::parse(br#"{"schema_version":1.0000000000000001}"#).unwrap_err(),
        "InvalidNumber"
    );
    assert_eq!(
        content::registry::parse(br#"{"schema_version":"1_"}"#).unwrap_err(),
        "InvalidCharacter"
    );
    assert_eq!(
        content::registry::parse(br#"{"schema_version":4294967296}"#).unwrap_err(),
        "Overflow"
    );
}

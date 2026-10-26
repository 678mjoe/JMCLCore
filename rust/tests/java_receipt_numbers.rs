//! Persisted receipt reads/listing only. No executable is created or invoked;
//! no installer, archive, acquisition, detection, selection or removal operation.
use serde_json::{Value, json};
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
            info.dwVolumeSerialNumber.into(),
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        )
    }
}
fn run(binary: &str, args: &[&str], input: Option<&[u8]>) -> (i32, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    drop(child.stdin.take());
    let o = child.wait_with_output().unwrap();
    (o.status.code().unwrap(), o.stdout, o.stderr)
}
fn fixture() -> Value {
    json!({"schema_version":1,"provider":"mojang","id":"fixture","platform":"linux","major_version":17,"java_home":"","executable":"bin/java","source_url":"https://example.invalid/runtime","files_total":1,"bytes_total":7,"installed_at_ms":123})
}
#[test]
fn persisted_receipt_precision_and_legacy_grammar() {
    use jmcl_core_rust::java::receipt;
    for (field, token, valid) in [
        ("schema_version", "1.0000000000000001", false),
        ("schema_version", "\"1.0\"", true),
        ("major_version", "\"+1__7\"", true),
        ("bytes_total", "\"18446744073709551615.0\"", true),
        ("installed_at_ms", "\"-9223372036854775808.0\"", true),
        ("installed_at_ms", "\"9223372036854775808\"", false),
        ("installed_at_ms", "\"-+1\"", false),
    ] {
        let mut v = fixture();
        v[field] = json!("TOKEN");
        let raw = serde_json::to_string(&v)
            .unwrap()
            .replace("\"TOKEN\"", token);
        assert_eq!(
            receipt::parse(raw.as_bytes()).is_ok(),
            valid,
            "{field}={token}"
        );
    }
}
#[test]
#[ignore = "requires pinned Zig executable; receipt listing only"]
fn persisted_receipt_numeric_list_matches_and_never_changes_disk() {
    let zig = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let rust = env!("CARGO_BIN_EXE_jmcl-core-rust");
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let runtime = root.join("runtimes/mojang-fixture-linux");
    fs::create_dir_all(&runtime).unwrap();
    // The executable field intentionally names a nonexistent path: listing must
    // read receipt metadata, not probe or launch any executable.
    let path = runtime.join(".jmcl-runtime.json");
    let sibling = root.join("receipt-sibling");
    let tokens = [
        "1",
        "1.0",
        "1e0",
        "1.0000000000000001",
        "\"1.0000000000000001\"",
        "\"+1__0\"",
        "\"1.0\"",
        "\"0x1.0p0\"",
        "\"1e+\"",
        "\"1_.0\"",
        "-0",
        "\"-0\"",
        "-1",
        "\"-1\"",
        "65535",
        "65536",
        "4294967295",
        "4294967296",
        "18446744073709551615",
        "\"18446744073709551615.0\"",
        "18446744073709551616",
        "-9223372036854775808",
        "\"-9223372036854775808.0\"",
        "9223372036854775807",
        "\"9223372036854775807.0\"",
        "9223372036854775808",
        "1e400",
        "null",
        "true",
        "[]",
        "{}",
        "\"\"",
        "\"-+1\"",
        "\"+_1\"",
        "\"1\\u002e0000000000000001\"",
    ];
    let mut count = 0;
    for field in [
        "schema_version",
        "major_version",
        "files_total",
        "bytes_total",
        "installed_at_ms",
    ] {
        for token in tokens {
            let mut v = fixture();
            v[field] = json!("TOKEN");
            let raw = serde_json::to_string(&v)
                .unwrap()
                .replace("\"TOKEN\"", token);
            if sibling.exists() {
                fs::remove_file(&sibling).unwrap();
            }
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            fs::write(&path, &raw).unwrap();
            fs::hard_link(&path, &sibling).unwrap();
            let before = identity(&path);
            let request = format!(
                "{}\n",
                json!({"protocol":1,"id":"receipt","method":"java.runtime.list","params":{"store_directory":root}})
            );
            let old = run(&zig, &["rpc"], Some(request.as_bytes()));
            let new = run(rust, &["rpc"], Some(request.as_bytes()));
            assert_eq!(new, old, "RPC {field}={token}");
            assert_eq!(new.0, 0);
            let doc: Value = serde_json::from_slice(&new.1).unwrap();
            let entries = doc["result"]["runtimes"].as_array().expect("list result");
            let parsed = jmcl_core_rust::java::receipt::parse(raw.as_bytes());
            assert_eq!(entries.len(), usize::from(parsed.is_ok()));
            if let Ok(r) = parsed {
                assert_eq!(entries[0], r.rpc("mojang-fixture-linux"));
            }
            for json in [false, true] {
                let mut args = vec!["java", "runtime", "list", "--store", root.to_str().unwrap()];
                if json {
                    args.push("--json");
                }
                let old = run(&zig, &args, None);
                let new = run(rust, &args, None);
                assert_eq!(new, old, "CLI {field}={token}");
                assert_eq!(new.0, 0);
            }
            for p in [&path, &sibling] {
                assert_eq!(fs::read(p).unwrap(), raw.as_bytes());
                assert_eq!(identity(p), before);
            }
            assert_eq!(fs::read_dir(&runtime).unwrap().count(), 1);
            assert!(!runtime.join("bin").exists());
            count += 1;
        }
    }
    eprintln!(
        "{count} persisted receipt fixtures, {} exact RPC/CLI comparisons; no executable or installer; disk preserved",
        count * 3
    );
}

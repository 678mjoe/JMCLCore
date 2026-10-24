//! Offline wire/CLI/disk parity. Every managed download is seeded in private CAS.
use jmcl_core_rust::modpack::{cli, rpc};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn archive(path: &Path, entries: &[(&str, Vec<u8>)]) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, b) in entries {
        z.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        z.write_all(b).unwrap();
    }
    z.finish().unwrap();
}
fn setup(root: &Path, mode: &str) -> Value {
    fs::create_dir_all(root).unwrap();
    let hash = format!("{:x}", Sha1::digest(b"hello"));
    let mut index = json!({"formatVersion":1,"game":"minecraft","name":"Fixture","versionId":"v1","dependencies":{"minecraft":"1.21"},"files":[]});
    let mut entries = vec![];
    match mode {
        "cached" | "private" | "traversal" => {
            index["files"] = json!([{"path":"mods/a.jar","hashes":{"sha1":hash},"fileSize":5,"downloads":["https://fixture.invalid/a.jar"]}]);
            let dir = root.join("store/objects/sha1").join(&hash[..2]);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(&hash), b"hello").unwrap();
            if mode == "private" {
                entries.push(("overrides/mods/a.jar", b"private".to_vec()));
            }
            if mode == "traversal" {
                entries.push(("overrides/../bad", b"bad".to_vec()));
            }
        }
        "unsupported" => index["formatVersion"] = json!(2),
        "quilt" => index["dependencies"]["quilt-loader"] = json!("1"),
        "conflict" => {
            index["dependencies"]["fabric-loader"] = json!("1");
            index["dependencies"]["forge"] = json!("1");
        }
        "raw" => entries.push(("overrides/rawX", b"private".to_vec())),
        "badname" => index["name"] = json!(""),
        _ => {}
    }
    if mode == "cfempty" || mode == "cfoptional" {
        entries.push(("manifest.json",serde_json::to_vec(&json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"CF Fixture","minecraft":{"version":"1.21"},"files":if mode=="cfoptional"{json!([{"projectID":1,"fileID":2,"required":false}])}else{json!([])}})).unwrap()));
    } else if mode == "unknown" {
        entries.push(("unknown", vec![]));
    } else if mode == "curseforge" {
        entries.push(("manifest.json", b"bad".to_vec()));
    } else if mode == "malformed" {
        entries.push(("modrinth.index.json", b"bad".to_vec()));
    } else {
        entries.push(("modrinth.index.json", serde_json::to_vec(&index).unwrap()));
    }
    archive(&root.join("pack.zip"), &entries);
    if mode == "raw" {
        let path = root.join("pack.zip");
        let mut bytes = fs::read(&path).unwrap();
        let positions: Vec<_> = bytes
            .windows(14)
            .enumerate()
            .filter_map(|(i, b)| (b == b"overrides/rawX").then_some(i))
            .collect();
        assert_eq!(positions.len(), 2);
        for i in positions {
            bytes[i + 13] = 0xff;
        }
        fs::write(path, bytes).unwrap();
    }
    if mode == "existing" {
        fs::create_dir_all(root.join("instances/test")).unwrap();
    }
    let mut params = json!({"directory":root.join("instances"),"store_directory":root.join("store"),"id":"test","file":root.join(if mode=="missing"{"missing.zip"}else{"pack.zip"}),"workers":1,"retries":0});
    if mode.starts_with("cf") {
        params["api_key"] = json!("fixture-not-a-real-key");
    }
    params
}
fn zig(exe: &std::ffi::OsStr, args: &[String], input: Option<Vec<u8>>) -> std::process::Output {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(b) = input {
        child.stdin.take().unwrap().write_all(&b).unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, at: &Path, v: &mut Vec<(String, Vec<u8>)>) {
        if !at.exists() {
            return;
        }
        for e in fs::read_dir(at).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(base, &e.path(), v)
            } else {
                v.push((
                    e.path()
                        .strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .into(),
                    fs::read(e.path()).unwrap(),
                ))
            }
        }
    }
    let mut v = vec![];
    walk(root, &root.join("instances"), &mut v);
    v.sort();
    v
}
#[test]
#[ignore = "requires pinned oracle; mandatory in integrated CI"]
fn raw_wire_cli_and_disk_oracle() {
    let exe = std::env::var_os("JMCL_ZIG_ORACLE").expect("set required pinned JMCL_ZIG_ORACLE");
    let tmp = tempfile::tempdir().unwrap();
    let mut count = 0;
    for mode in [
        "raw",
        "empty",
        "cached",
        "private",
        "traversal",
        "unsupported",
        "quilt",
        "conflict",
        "badname",
        "unknown",
        "curseforge",
        "malformed",
        "existing",
        "missing",
        "cfempty",
        "cfoptional",
    ] {
        if mode == "raw" && !cfg!(unix) {
            continue;
        }
        let zroot = tmp.path().join(format!("{mode}-zig"));
        let rroot = tmp.path().join(format!("{mode}-rust"));
        let zp = setup(&zroot, mode);
        let rp = setup(&rroot, mode);
        let mut input = serde_json::to_vec(
            &json!({"protocol":1,"id":"fixture","method":"modpack.install","params":zp}),
        )
        .unwrap();
        input.push(b'\n');
        let zo = zig(&exe, &["rpc".into()], Some(input));
        assert!(zo.status.success());
        let raw = serde_json::value::to_raw_value(&rp).unwrap();
        let mut ro = vec![];
        rpc::handle("modpack.install", Some(&raw), "fixture", &mut ro)
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&ro),
            String::from_utf8_lossy(&zo.stdout),
            "RPC {mode}"
        );
        assert_eq!(snapshot(&zroot), snapshot(&rroot), "disk {mode}");
        #[cfg(unix)]
        if mode == "raw" {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(
                fs::read(
                    rroot
                        .join("instances/test/.minecraft")
                        .join(std::ffi::OsStr::from_bytes(b"raw\xff"))
                )
                .unwrap(),
                b"private"
            );
        }
        count += 1;
        let zroot = tmp.path().join(format!("{mode}-cli-zig"));
        let rroot = tmp.path().join(format!("{mode}-cli-rust"));
        let zp = setup(&zroot, mode);
        let rp = setup(&rroot, mode);
        let args = |p: &Value| {
            let mut a = vec![
                "install".into(),
                "test".into(),
                "--file".into(),
                p["file"].as_str().unwrap().into(),
                "--instances-directory".into(),
                p["directory"].as_str().unwrap().into(),
                "--store-directory".into(),
                p["store_directory"].as_str().unwrap().into(),
                "--workers".into(),
                "1".into(),
                "--retries".into(),
                "0".into(),
                "--api-key".into(),
                "fixture-not-a-real-key".into(),
            ];
            if mode == "curseforge" {
                a.truncate(a.len() - 2);
            }
            a
        };
        let mut za = vec!["instance".into(), "modpack".into()];
        za.extend(args(&zp));
        let zo = zig(&exe, &za, None);
        let mut out = vec![];
        let mut err = vec![];
        let rc = cli::run(&args(&rp), &mut out, &mut err).unwrap();
        assert_eq!(rc as i32, zo.status.code().unwrap(), "CLI status {mode}");
        assert_eq!(out, zo.stdout, "CLI stdout {mode}");
        assert_eq!(
            String::from_utf8_lossy(&err),
            String::from_utf8_lossy(&zo.stderr),
            "CLI stderr {mode}"
        );
        assert_eq!(snapshot(&zroot), snapshot(&rroot), "CLI disk {mode}");
        count += 1;
    }
    for p in [
        None,
        Some(json!({})),
        Some(json!(null)),
        Some(json!({"directory":"","id":"a","file":"x"})),
        Some(json!({"directory":"x","id":"../bad","file":"x"})),
        Some(json!({"directory":"x","id":"a","file":"x","api_key":"fake key"})),
        Some(json!({"directory":"x","id":"a","file":"x","workers":33})),
        Some(json!({"directory":"x","id":"a","file":"x","retries":6})),
    ] {
        let mut req = json!({"protocol":1,"id":"fixture","method":"modpack.install"});
        if let Some(p) = &p {
            req["params"] = p.clone();
        }
        let mut b = serde_json::to_vec(&req).unwrap();
        b.push(b'\n');
        let zo = zig(&exe, &["rpc".into()], Some(b));
        let raw = p
            .as_ref()
            .map(|p| serde_json::value::to_raw_value(p).unwrap());
        let mut out = vec![];
        rpc::handle("modpack.install", raw.as_deref(), "fixture", &mut out)
            .unwrap()
            .unwrap();
        assert_eq!(out, zo.stdout, "params {p:?}");
        count += 1;
    }
    eprintln!("matched {count} raw RPC/CLI and 32 disk fixtures");
}

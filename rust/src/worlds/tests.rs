use super::*;
use serde_json::json;
use std::{
    fs as stdfs,
    process::{Command, Stdio},
};
fn fixture() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    stdfs::create_dir_all(t.path().join("saves/Alpha/region")).unwrap();
    stdfs::write(t.path().join("saves/Alpha/level.dat"), b"not-nbt").unwrap();
    stdfs::write(t.path().join("saves/Alpha/region/r.0.0.mca"), b"mutable").unwrap();
    t
}
fn path(t: &tempfile::TempDir) -> &str {
    t.path().to_str().unwrap()
}
fn call(m: &str, v: Value) -> std::result::Result<Value, crate::protocol::RpcError> {
    let p = serde_json::value::to_raw_value(&v).unwrap();
    handle(m, Some(&p)).unwrap()
}
fn valid_level() -> Vec<u8> {
    let doc = nbt::Document {
        name: vec![],
        root: nbt::Tag::Compound(vec![(
            b"Data".to_vec(),
            nbt::Tag::Compound(vec![
                (b"LevelName".to_vec(), nbt::Tag::String(b"Alpha".to_vec())),
                (b"GameType".to_vec(), nbt::Tag::Int(1)),
                (
                    b"Version".to_vec(),
                    nbt::Tag::Compound(vec![
                        (b"Name".to_vec(), nbt::Tag::String(b"1.21.1".to_vec())),
                        (b"Id".to_vec(), nbt::Tag::Int(3955)),
                    ]),
                ),
                (b"unknown".to_vec(), nbt::Tag::Longs(vec![5, 8])),
            ]),
        )]),
    };
    let mut enc = GzEncoder::new(vec![], Compression::default());
    enc.write_all(&nbt::serialize(&doc).unwrap()).unwrap();
    enc.finish().unwrap()
}
#[test]
fn metadata_and_rename_preserve_unknown_nbt() {
    let t = fixture();
    stdfs::write(t.path().join("saves/Alpha/level.dat"), valid_level()).unwrap();
    let got = get(path(&t), "Alpha", Some("1.21.1")).unwrap();
    assert_eq!(got["world"]["version_relation"], "same");
    assert_eq!(got["world"]["game_mode"], "creative");
    rename(path(&t), "Alpha", "Beta", None).unwrap();
    let doc = decode_level(&stdfs::read(t.path().join("saves/Beta/level.dat")).unwrap()).unwrap();
    assert_eq!(
        doc.root
            .get("Data")
            .unwrap()
            .get("LevelName")
            .unwrap()
            .text(),
        Some("Beta")
    );
    assert_eq!(
        doc.root.get("Data").unwrap().get("unknown"),
        Some(&nbt::Tag::Longs(vec![5, 8]))
    );
}
#[test]
fn mutable_duplicates_are_copies_not_hardlinks() {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    let t = fixture();
    duplicate(path(&t), "Alpha", "Beta", None).unwrap();
    let a = t.path().join("saves/Alpha/region/r.0.0.mca");
    let b = t.path().join("saves/Beta/region/r.0.0.mca");
    #[cfg(unix)]
    assert_ne!(
        stdfs::metadata(&a).unwrap().ino(),
        stdfs::metadata(&b).unwrap().ino()
    );
    stdfs::write(&b, b"changed").unwrap();
    delete(path(&t), "Beta").unwrap();
    assert_eq!(stdfs::read(&a).unwrap(), b"mutable");
}
#[test]
fn locked_restore_refuses_and_stale_lock_allows() {
    let t = fixture();
    let b = archive::backup(path(&t), "Alpha", Some("hello world!")).unwrap();
    let backup = b["backup"].as_str().unwrap();
    let lock = stdfs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(t.path().join("saves/Alpha/session.lock"))
        .unwrap();
    lock.lock().unwrap();
    let out = archive::restore(path(&t), backup, None, true, None);
    assert!(matches!(out, Err(Error::WorldLocked)));
    assert_eq!(
        stdfs::read(t.path().join("saves/Alpha/level.dat")).unwrap(),
        b"not-nbt"
    );
    let entries = stdfs::read_dir(t.path().join("saves")).unwrap().count();
    assert_eq!(entries, 1);
    lock.unlock().unwrap();
    drop(lock);
    archive::restore(path(&t), backup, None, true, None).unwrap();
    assert!(!t.path().join("saves/Alpha/session.lock").exists());
}
#[test]
fn export_excludes_registry_lock_and_temporary_files() {
    let t = fixture();
    let a = t.path().join("saves/Alpha");
    stdfs::create_dir(a.join(".jmcl")).unwrap();
    stdfs::write(a.join(".jmcl/registry.json"), b"private").unwrap();
    stdfs::write(a.join("session.lock"), b"stale").unwrap();
    stdfs::write(a.join("x.jmcl-copy-junk"), b"temp").unwrap();
    let p = t.path().join("world.zip");
    let out = archive::export(path(&t), "Alpha", p.to_str().unwrap()).unwrap();
    assert_eq!(
        out["size_bytes"].as_u64().unwrap(),
        stdfs::metadata(&p).unwrap().len()
    );
    let mut z = zip::ZipArchive::new(stdfs::File::open(&p).unwrap()).unwrap();
    assert_eq!(out["files"], 2);
    for i in 0..z.len() {
        let f = z.by_index(i).unwrap();
        assert!(!f.name().contains("session.lock") && !f.name().contains(".jmcl"));
    }
}
#[test]
fn malicious_zip_paths_do_not_escape() {
    let t = fixture();
    for evil in [
        "../escape",
        "/absolute",
        "a/../../outside",
        "C:/escape",
        "a\\escape",
    ] {
        let p = t.path().join("evil.zip");
        let mut z = zip::ZipWriter::new(stdfs::File::create(&p).unwrap());
        z.start_file(evil, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"bad").unwrap();
        z.finish().unwrap();
        assert!(matches!(
            archive::import(path(&t), p.to_str().unwrap(), Some("Bad"), false, None),
            Err(Error::ArchiveInvalid)
        ));
        assert!(!t.path().join("saves/Bad").exists());
    }
}
#[test]
fn invalid_params_and_names_match_contract() {
    assert_eq!(
        call("worlds.list", json!({})).unwrap_err().message,
        "worlds list parameters are invalid"
    );
    assert_eq!(
        call("worlds.import", json!({"file":""}))
            .unwrap_err()
            .message,
        "world import requires directory and file"
    );
    assert_eq!(
        sanitize_label("  hello / world..."),
        Some("hello-world".into())
    );
    assert!(validate_name("../x").is_err());
    assert_eq!(
        backup_world_name("My-world-20261024-120000-label.zip"),
        Some("My-world")
    );
}
#[test]
fn ordinary_zip_roundtrip_preserves_files() {
    let t = fixture();
    let p = t.path().join("world.zip");
    archive::export(path(&t), "Alpha", p.to_str().unwrap()).unwrap();
    archive::import(path(&t), p.to_str().unwrap(), Some("Beta"), false, None).unwrap();
    assert_eq!(
        stdfs::read(t.path().join("saves/Beta/region/r.0.0.mca")).unwrap(),
        b"mutable"
    );
    assert!(matches!(
        archive::import(path(&t), p.to_str().unwrap(), Some("Beta"), false, None),
        Err(Error::WorldExists)
    ));
}
fn oracle(binary: &str, m: &str, params: Value) -> Value {
    let mut child = Command::new(binary)
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"protocol":1,"id":"w","method":m,"params":params})
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["event"], "result", "{v}");
    v["result"].clone()
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; CI must run explicitly"]
fn zig_rust_metadata_and_archive_interoperability() {
    let binary = std::env::var("JMCL_ZIG_ORACLE").expect("oracle required");
    let t = fixture();
    stdfs::write(t.path().join("saves/Alpha/level.dat"), valid_level()).unwrap();
    for (m, p) in [
        (
            "worlds.list",
            json!({"directory":path(&t),"minecraft_version":"1.21.1"}),
        ),
        (
            "worlds.get",
            json!({"directory":path(&t),"world":"Alpha","minecraft_version":"1.21.1"}),
        ),
    ] {
        assert_eq!(oracle(&binary, m, p.clone()), call(m, p).unwrap());
    }
    let zigzip = t.path().join("zig.zip");
    let z = oracle(
        &binary,
        "worlds.export",
        json!({"directory":path(&t),"world":"Alpha","file":zigzip}),
    );
    assert_eq!(
        z["size_bytes"].as_u64().unwrap(),
        stdfs::metadata(&zigzip).unwrap().len()
    );
    archive::import(
        path(&t),
        zigzip.to_str().unwrap(),
        Some("FromZig"),
        false,
        None,
    )
    .unwrap();
    let rustzip = t.path().join("rust.zip");
    let r = archive::export(path(&t), "Alpha", rustzip.to_str().unwrap()).unwrap();
    assert_eq!(
        r["size_bytes"].as_u64().unwrap(),
        stdfs::metadata(&rustzip).unwrap().len()
    );
    oracle(
        &binary,
        "worlds.import",
        json!({"directory":path(&t),"file":rustzip,"name":"FromRust"}),
    );
    for name in ["FromZig", "FromRust"] {
        assert_eq!(
            stdfs::read(t.path().join(format!("saves/{name}/region/r.0.0.mca"))).unwrap(),
            b"mutable"
        );
        let p = json!({"directory":path(&t),"world":name});
        assert_eq!(
            oracle(&binary, "worlds.get", p.clone()),
            call("worlds.get", p).unwrap()
        );
    }
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; CI must run explicitly"]
fn zig_rust_errors_and_backup_listing_match() {
    let binary = std::env::var("JMCL_ZIG_ORACLE").expect("oracle required");
    let t = fixture();
    let cases = [
        ("worlds.list", json!({})),
        (
            "worlds.get",
            json!({"directory":path(&t),"world":"Missing"}),
        ),
        ("worlds.get", json!({"directory":path(&t),"world":".."})),
        ("worlds.delete", json!({"directory":path(&t),"world":""})),
        (
            "worlds.rename",
            json!({"directory":path(&t),"world":"Alpha","new_name":"Alpha"}),
        ),
        (
            "worlds.import",
            json!({"directory":path(&t),"file":"missing.zip"}),
        ),
        (
            "worlds.restore",
            json!({"directory":path(&t),"backup":"../../bad.zip"}),
        ),
        (
            "worlds.export",
            json!({"directory":path(&t),"world":"Alpha","file":"","minecraft_version":42}),
        ),
    ];
    for (m, p) in cases {
        let mut child = Command::new(&binary)
            .arg("rpc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"protocol":1,"id":"e","method":m,"params":p})
        )
        .unwrap();
        let out = child.wait_with_output().unwrap();
        let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
        let err = call(m, p.clone()).expect_err(m);
        assert_eq!(
            actual["error"],
            json!({"code":err.code,"message":err.message}),
            "{m}: {p}"
        );
    }
    let b = archive::backup(path(&t), "Alpha", Some("a label!")).unwrap();
    let list = backups(path(&t)).unwrap();
    assert_eq!(
        list,
        oracle(&binary, "worlds.backups", json!({"directory":path(&t)}))
    );
    assert_eq!(list["backups"][0]["file"], b["backup"]);
}
#[test]
#[ignore = "records pending malformed-CRC policy, not a parity pass"]
fn known_gap_crc_oracle_accepts_rust_rejects() {
    let binary = std::env::var("JMCL_ZIG_ORACLE").expect("oracle required");
    let t = fixture();
    let p = t.path().join("crc.zip");
    let mut zip = zip::ZipWriter::new(stdfs::File::create(&p).unwrap());
    zip.start_file(
        "world/level.dat",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(b"data").unwrap();
    zip.finish().unwrap();
    let mut bytes = stdfs::read(&p).unwrap();
    let local = bytes.windows(4).position(|s| s == b"PK\x03\x04").unwrap();
    let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    bytes[local + 14] ^= 1;
    bytes[central + 16] ^= 1;
    stdfs::write(&p, bytes).unwrap();
    let result = oracle(
        &binary,
        "worlds.import",
        json!({"directory":path(&t),"file":p,"name":"Oracle"}),
    );
    assert_eq!(result["world"]["name"], "Oracle");
    assert!(matches!(
        archive::import(path(&t), p.to_str().unwrap(), Some("Rust"), false, None),
        Err(Error::ArchiveInvalid)
    ));
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; CI must run explicitly"]
#[cfg(unix)]
fn zig_rust_level_dat_read_alias() {
    use std::os::unix::fs::symlink;
    let binary = std::env::var("JMCL_ZIG_ORACLE").expect("oracle required");
    let t = fixture();
    let external = tempfile::tempdir().unwrap();
    stdfs::write(external.path().join("level.dat"), valid_level()).unwrap();
    let leaf = t.path().join("saves/Alpha/level.dat");
    stdfs::remove_file(&leaf).unwrap();
    symlink(external.path().join("level.dat"), &leaf).unwrap();
    let old = oracle(
        &binary,
        "worlds.get",
        json!({"directory":path(&t),"world":"Alpha"}),
    );
    let new = get(path(&t), "Alpha", None).unwrap();
    assert_eq!(old["world"]["level_name"], "Alpha");
    assert_eq!(old, new);
    let before = stdfs::read(external.path().join("level.dat")).unwrap();
    rename(path(&t), "Alpha", "Beta", None).unwrap();
    assert_eq!(
        stdfs::read(external.path().join("level.dat")).unwrap(),
        before
    );
    assert!(
        !stdfs::symlink_metadata(t.path().join("saves/Beta/level.dat"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        get(path(&t), "Beta", None).unwrap()["world"]["level_name"],
        "Beta"
    );
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; CI must run explicitly"]
fn zig_rust_instance_world_cli_matches() {
    let binary = std::env::var("JMCL_ZIG_ORACLE").expect("oracle required");
    let t = fixture();
    let root = t.path().join("instances");
    let game = root.join("demo/.minecraft");
    stdfs::create_dir_all(&game).unwrap();
    stdfs::rename(t.path().join("saves"), game.join("saves")).unwrap();
    stdfs::write(root.join("demo/instance.json"),serde_json::to_vec(&json!({"schema_version":1,"id":"demo","name":"Demo","version_id":"1.21.1","source":"official"})).unwrap()).unwrap();
    let rootarg = root.to_str().unwrap();
    let cases = vec![
        vec!["list", "demo", "--instances-directory", rootarg],
        vec!["list", "demo", "--instances-directory", rootarg, "--json"],
        vec!["show", "demo", "Alpha", "--instances-directory", rootarg],
        vec![
            "show",
            "demo",
            "Alpha",
            "--instances-directory",
            rootarg,
            "--json",
        ],
        vec!["show", "demo", "Missing", "--instances-directory", rootarg],
        vec!["show", "demo"],
        vec!["show"],
        vec!["bogus"],
        vec!["list", "missing", "--instances-directory", rootarg],
        vec![
            "backups",
            "demo",
            "--instances-directory",
            rootarg,
            "--json",
        ],
    ];
    for args in cases {
        let old = Command::new(&binary)
            .args(["instance", "worlds"])
            .args(&args)
            .output()
            .unwrap();
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let mut out = vec![];
        let mut err = vec![];
        let code = super::cli::cli(&args, &mut out, &mut err).unwrap().unwrap();
        assert_eq!(old.status.code(), Some(code as i32), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&old.stdout),
            String::from_utf8_lossy(&out),
            "stdout {args:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&old.stderr),
            String::from_utf8_lossy(&err),
            "stderr {args:?}"
        );
    }
    stdfs::write(game.join("saves/Alpha/level.dat"), valid_level()).unwrap();
    let args = vec![
        "show".into(),
        "demo".into(),
        "Alpha".into(),
        "--instances-directory".into(),
        rootarg.into(),
    ];
    let old = Command::new(&binary)
        .args(["instance", "worlds"])
        .args(&args)
        .output()
        .unwrap();
    let mut out = vec![];
    let mut err = vec![];
    assert_eq!(
        super::cli::cli(&args, &mut out, &mut err).unwrap().unwrap(),
        0
    );
    assert_eq!(old.stdout, out);
    assert_eq!(old.stderr, err);
}
#[test]
fn failed_publication_restores_old_world() {
    let t = fixture();
    let s = saves(path(&t)).unwrap();
    let temp = unique(".jmcl-copy-", "Alpha").unwrap();
    s.new_child(&temp)
        .unwrap()
        .write_new("level.dat", b"new")
        .unwrap();
    let mut calls = 0;
    let result = archive::publish_staging(&s, &temp, "Alpha", true, &mut |old, new| {
        calls += 1;
        if calls == 2 {
            Err(Error::Io(io::Error::other("injected publication failure")))
        } else {
            s.rename(old, &s, new)
        }
    });
    assert!(result.is_err());
    assert_eq!(
        stdfs::read(t.path().join("saves/Alpha/level.dat")).unwrap(),
        b"not-nbt"
    );
    assert_eq!(
        s.child(&temp, false)
            .unwrap()
            .read("level.dat", 32)
            .unwrap(),
        b"new"
    );
    s.remove_tree(&temp).unwrap();
    assert_eq!(stdfs::read_dir(t.path().join("saves")).unwrap().count(), 1);
}
#[test]
fn malformed_nbt_limits_and_truncation() {
    let mut deep = vec![10, 0, 0];
    for _ in 0..65 {
        deep.extend_from_slice(&[10, 0, 0]);
    }
    deep.extend(std::iter::repeat_n(0, 66));
    assert!(nbt::parse(&deep).is_err());
    for tag in [7u8, 9, 11, 12] {
        let mut bytes = vec![10, 0, 0, tag, 0, 0];
        if tag == 9 {
            bytes.push(1)
        }
        bytes.extend_from_slice(&(-1i32).to_be_bytes());
        bytes.push(0);
        assert!(nbt::parse(&bytes).is_err());
    }
    let d = decode_level(&valid_level()).unwrap();
    let b = nbt::serialize(&d).unwrap();
    for len in 0..b.len() {
        assert!(nbt::parse(&b[..len]).is_err(), "truncation {len}");
    }
    let bytes = vec![0; 16 * 1024 * 1024 + 1];
    assert!(nbt::parse(&bytes).is_err());
}
#[test]
fn archive_declared_size_and_malformed_structure_rejected() {
    let t = fixture();
    let p = t.path().join("bad.zip");
    let mut z = zip::ZipWriter::new(stdfs::File::create(&p).unwrap());
    z.start_file(
        "Alpha/level.dat",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    z.write_all(b"data").unwrap();
    z.finish().unwrap();
    let valid = stdfs::read(&p).unwrap();
    let central = valid.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    let mut huge = valid.clone();
    huge[central + 24..central + 28].copy_from_slice(&((MAX_ARCHIVE + 1) as u32).to_le_bytes());
    stdfs::write(&p, huge).unwrap();
    assert!(archive::import(path(&t), p.to_str().unwrap(), Some("Huge"), false, None).is_err());
    assert!(!t.path().join("saves/Huge").exists());
    for bytes in [
        vec![],
        valid[..valid.len() - 10].to_vec(),
        b"notzip".to_vec(),
    ] {
        stdfs::write(&p, bytes).unwrap();
        assert!(matches!(
            archive::import(path(&t), p.to_str().unwrap(), Some("Broken"), false, None),
            Err(Error::ArchiveInvalid)
        ));
    }
    assert_eq!(stdfs::read_dir(t.path().join("saves")).unwrap().count(), 1);
}
#[test]
#[cfg(unix)]
fn symlink_entries_rejected_on_copy_and_export() {
    use std::os::unix::fs::symlink;
    let t = fixture();
    let outside = tempfile::tempdir().unwrap();
    stdfs::write(outside.path().join("secret"), b"outside").unwrap();
    symlink(
        outside.path().join("secret"),
        t.path().join("saves/Alpha/link"),
    )
    .unwrap();
    assert!(matches!(
        duplicate(path(&t), "Alpha", "Beta", None),
        Err(Error::UnsupportedEntry)
    ));
    let zip = t.path().join("out.zip");
    assert!(matches!(
        archive::export(path(&t), "Alpha", zip.to_str().unwrap()),
        Err(Error::UnsupportedEntry)
    ));
    assert!(!t.path().join("saves/Beta").exists());
    assert!(!zip.exists());
    assert_eq!(
        stdfs::read(outside.path().join("secret")).unwrap(),
        b"outside"
    );
}

#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; CI must run explicitly"]
fn zig_rust_world_cli_mutation_results_match() {
    let binary = std::env::var("JMCL_ZIG_ORACLE").expect("oracle required");
    let t = fixture();
    let root = t.path().join("instances");
    let game = root.join("demo/.minecraft");
    stdfs::create_dir_all(&game).unwrap();
    stdfs::write(root.join("demo/instance.json"),serde_json::to_vec(&json!({"schema_version":1,"id":"demo","name":"Demo","version_id":"1.21.1","source":"official"})).unwrap()).unwrap();
    let archive = t.path().join("seed.zip");
    archive::export(path(&t), "Alpha", archive.to_str().unwrap()).unwrap();
    let backup = "Alpha--123--test.zip";
    stdfs::create_dir_all(game.join("backups")).unwrap();
    let cases = vec![
        vec!["rename", "demo", "Alpha", "Beta"],
        vec!["duplicate", "demo", "Alpha", "Beta"],
        vec!["delete", "demo", "Alpha"],
        vec![
            "import",
            "demo",
            archive.to_str().unwrap(),
            "--name",
            "Imported",
        ],
        vec!["restore", "demo", backup, "--name", "Restored"],
        vec!["backups-delete", "demo", backup],
        vec!["rename", "demo", "Alpha", "../escape"],
        vec!["duplicate", "demo", "Missing", "Beta"],
        vec!["delete", "demo", "Missing"],
    ];
    for (index, case) in cases.into_iter().enumerate() {
        let mut results = vec![];
        for zig in [true, false] {
            if game.join("saves").exists() {
                stdfs::remove_dir_all(game.join("saves")).unwrap()
            }
            stdfs::create_dir_all(game.join("saves/Alpha/region")).unwrap();
            stdfs::write(game.join("saves/Alpha/level.dat"), valid_level()).unwrap();
            stdfs::write(game.join("saves/Alpha/region/r.0.0.mca"), b"mutable").unwrap();
            stdfs::copy(&archive, game.join("backups").join(backup)).unwrap();
            let mut args = case.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            args.extend([
                "--instances-directory".into(),
                root.to_str().unwrap().into(),
            ]);
            let result = if zig {
                let p = Command::new(&binary)
                    .args(["instance", "worlds"])
                    .args(&args)
                    .output()
                    .unwrap();
                (p.status.code().unwrap() as u8, p.stdout, p.stderr)
            } else {
                let (mut out, mut err) = (vec![], vec![]);
                let status = super::cli::cli(&args, &mut out, &mut err).unwrap().unwrap();
                (status, out, err)
            };
            results.push(result);
        }
        assert_eq!(
            results[0].0,
            if index < 6 { 0 } else { 1 },
            "{case:?}: {:?}",
            results[0]
        );
        assert_eq!(results[0], results[1], "{case:?}");
    }
}

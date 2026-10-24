#[cfg(unix)]
#[path = "support/raw_names.rs"]
mod raw_names;
use jmcl_core_rust::{
    install,
    metadata::Source,
    plan::{AssetProjection, File, NativeExtraction, Plan, Target, Usage},
    store::ArtifactRoot,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{fs, io::Write, process::Command};
fn plan() -> Plan {
    Plan {
        version_id: "fixture".into(),
        source: Source::Official,
        target: Target {
            os: "linux".into(),
            arch: "x86_64".into(),
            os_version: None,
        },
        files: vec![],
        classpath: vec![],
        native_extractions: vec![],
        asset_projections: vec![],
        total_download_size: 0,
        java_major_version: 0,
        loader: "none".into(),
    }
}
fn planned(path: &str, b: &[u8]) -> File {
    File {
        path: path.into(),
        url: "https://unused".into(),
        fallback_url: None,
        sha1: format!("{:x}", Sha1::digest(b)),
        size: b.len() as u64,
        usage: Usage::default(),
    }
}
#[derive(Clone)]
struct Entry {
    name: String,
    body: Vec<u8>,
    method: u16,
    attrs: u32,
    declared: Option<u32>,
}
fn entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        body: b"abc".to_vec(),
        method: 0,
        attrs: 0o100644 << 16,
        declared: None,
    }
}
fn archive(entries: &[Entry]) -> Vec<u8> {
    let mut b = vec![];
    let mut central = vec![];
    for e in entries {
        let data = if e.method == 8 {
            let mut z = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
            z.write_all(&e.body).unwrap();
            z.finish().unwrap()
        } else {
            e.body.clone()
        };
        let size = e.declared.unwrap_or(e.body.len() as u32);
        let offset = b.len() as u32;
        let mut h = vec![0; 30];
        h[..4].copy_from_slice(b"PK\x03\x04");
        h[4..6].copy_from_slice(&20u16.to_le_bytes());
        h[8..10].copy_from_slice(&e.method.to_le_bytes());
        // Deliberately inaccurate CRC, matching pinned native extractor behavior.
        h[18..22].copy_from_slice(&(data.len() as u32).to_le_bytes());
        h[22..26].copy_from_slice(&size.to_le_bytes());
        h[26..28].copy_from_slice(&(e.name.len() as u16).to_le_bytes());
        b.extend(&h);
        b.extend(e.name.as_bytes());
        b.extend(&data);
        let mut c = vec![0; 46];
        c[..4].copy_from_slice(b"PK\x01\x02");
        c[4..6].copy_from_slice(&0x314u16.to_le_bytes());
        c[6..8].copy_from_slice(&20u16.to_le_bytes());
        c[10..12].copy_from_slice(&e.method.to_le_bytes());
        c[20..24].copy_from_slice(&(data.len() as u32).to_le_bytes());
        c[24..28].copy_from_slice(&size.to_le_bytes());
        c[28..30].copy_from_slice(&(e.name.len() as u16).to_le_bytes());
        c[38..42].copy_from_slice(&e.attrs.to_le_bytes());
        c[42..46].copy_from_slice(&offset.to_le_bytes());
        central.extend(c);
        central.extend(e.name.as_bytes());
    }
    let offset = b.len() as u32;
    let size = central.len() as u32;
    b.extend(central);
    let mut end = vec![0; 22];
    end[..4].copy_from_slice(b"PK\x05\x06");
    end[8..10].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    end[10..12].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    end[12..16].copy_from_slice(&size.to_le_bytes());
    end[16..20].copy_from_slice(&offset.to_le_bytes());
    b.extend(end);
    b
}
fn fixtures() -> Vec<(&'static str, Vec<Entry>, Vec<String>)> {
    let mut deflated = entry("nested/native.so");
    deflated.method = 8;
    let mut symlink = entry("symlink");
    symlink.attrs = 0o120777 << 16;
    let mut huge = entry("huge");
    huge.declared = Some(256 * 1024 * 1024 + 1);
    let mut ratio = entry("ratio");
    ratio.declared = Some(3004);
    let mut truncated = entry("truncated");
    truncated.declared = Some(100);
    let mut unsupported = entry("other");
    unsupported.method = 12;
    vec![
        ("stored", vec![entry("native.bin")], vec![]),
        ("deflated", vec![deflated], vec![]),
        ("duplicate", vec![entry("x"), entry("x")], vec![]),
        (
            "duplicate-before-unsafe",
            vec![entry("x"), entry("x"), entry("../bad")],
            vec![],
        ),
        ("traversal", vec![entry("../escape")], vec![]),
        ("symlink", vec![symlink], vec![]),
        ("huge", vec![huge], vec![]),
        ("ratio", vec![ratio], vec![]),
        (
            "excluded",
            vec![entry("META-INF/nope"), entry("native.bin")],
            vec!["META-INF/".into()],
        ),
        ("truncated", vec![truncated], vec![]),
        ("unsupported", vec![unsupported], vec![]),
        ("empty", vec![], vec![]),
    ]
}
fn run_rust(root: &ArtifactRoot, p: &Plan) -> Value {
    match install::prepare(root, p) {
        Ok(v) => serde_json::to_value(v).unwrap(),
        Err(e) => json!({"error":e.name()}),
    }
}
fn normalize(v: &mut Value, dir: &std::path::Path) {
    if let Some(path) = v
        .get("native_directory")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        assert!(path.starts_with(".jmcl/runtime/natives/"));
        assert!(dir.join(&path).is_dir());
        let mut contents = std::collections::BTreeMap::new();
        fn walk(
            d: &std::path::Path,
            base: &std::path::Path,
            out: &mut std::collections::BTreeMap<String, Vec<u8>>,
        ) {
            for e in fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, base, out)
                } else {
                    out.insert(
                        p.strip_prefix(base)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .replace('\\', "/"),
                        fs::read(p).unwrap(),
                    );
                }
            }
        }
        walk(&dir.join(&path), &dir.join(&path), &mut contents);
        v["native_directory"] = json!("<native-directory>");
        v["contents"] = json!(contents);
    }
}
#[test]
fn native_fixtures_are_bounded_and_private() {
    for (name, entries, exclude) in fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(tmp.path()).unwrap();
        let b = archive(&entries);
        root.atomic_replace("native.jar", &b).unwrap();
        let mut p = plan();
        p.files.push(planned("native.jar", &b));
        p.native_extractions.push(NativeExtraction {
            archive_path: "native.jar".into(),
            exclude,
        });
        let v = run_rust(&root, &p);
        if ["stored", "deflated", "excluded", "empty"].contains(&name) {
            let path = v["native_directory"].as_str().unwrap();
            install::cleanup_native(&root, path).unwrap();
            assert!(!tmp.path().join(path).exists());
        } else {
            assert!(v.get("error").is_some(), "{name}: {v}");
        }
    }
}
#[test]
fn mutable_replacement_and_projection_do_not_mutate_shared_inode() {
    let tmp = tempfile::tempdir().unwrap();
    let root = ArtifactRoot::open(tmp.path()).unwrap();
    root.atomic_replace("object", b"abc").unwrap();
    let mut p = plan();
    p.files.push(planned("object", b"abc"));
    p.asset_projections.push(AssetProjection {
        source_path: "object".into(),
        destination_path: "assets/a".into(),
        kind: "resources".into(),
    });
    let v = install::prepare(&root, &p).unwrap();
    assert_eq!(v.projections_created, 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(tmp.path().join("object")).unwrap().ino(),
            fs::metadata(tmp.path().join("assets/a")).unwrap().ino()
        );
    }
    root.atomic_replace("assets/a", b"mutable").unwrap();
    assert_eq!(fs::read(tmp.path().join("object")).unwrap(), b"abc");
    let v = install::prepare(&root, &p).unwrap();
    assert_eq!(v.projections_created, 1);
    assert_eq!(fs::read(tmp.path().join("assets/a")).unwrap(), b"abc");
}
#[test]
#[ignore = "requires pinned Zig preparer fixture adapter"]
fn preparation_matches_pinned_zig() {
    let oracle = std::env::var("JMCL_ZIG_PREPARE_ORACLE").unwrap();
    for (name, entries, exclude) in fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let rust = tmp.path().join("rust");
        let zig = tmp.path().join("zig");
        let root = ArtifactRoot::open(&rust).unwrap();
        fs::create_dir(&zig).unwrap();
        let b = archive(&entries);
        root.atomic_replace("native.jar", &b).unwrap();
        fs::write(zig.join("native.jar"), &b).unwrap();
        let mut p = plan();
        p.files.push(planned("native.jar", &b));
        p.native_extractions.push(NativeExtraction {
            archive_path: "native.jar".into(),
            exclude,
        });
        let fixture = tmp.path().join("plan.json");
        fs::write(&fixture, serde_json::to_vec(&p).unwrap()).unwrap();
        let output = Command::new(&oracle)
            .arg(fixture)
            .arg(&zig)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut zv: Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut rv = run_rust(&root, &p);
        normalize(&mut zv, &zig);
        normalize(&mut rv, &rust);
        assert_eq!(rv, zv, "{name}");
    }
}

#[test]
fn failed_extraction_cleans_native_tree_and_never_modifies_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let root = ArtifactRoot::open(tmp.path()).unwrap();
    let mut bytes = archive(&[entry("first"), entry("second")]);
    // First local version differs from central, forcing failure after directory creation.
    bytes[4] = 19;
    root.atomic_replace("native.jar", &bytes).unwrap();
    let mut p = plan();
    p.files.push(planned("native.jar", &bytes));
    p.native_extractions.push(NativeExtraction {
        archive_path: "native.jar".into(),
        exclude: vec![],
    });
    assert_eq!(
        install::prepare(&root, &p).unwrap_err().name(),
        "ZipMismatchVersionNeeded"
    );
    assert_eq!(fs::read(tmp.path().join("native.jar")).unwrap(), bytes);
    assert_eq!(
        fs::read_dir(tmp.path().join(".jmcl/runtime/natives"))
            .unwrap()
            .count(),
        0
    );
    assert!(install::cleanup_native(&root, ".jmcl/runtime/natives/../../objects").is_err());
}

#[cfg(unix)]
#[test]
fn native_and_mutable_outputs_cannot_follow_descendant_symlinks() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = ArtifactRoot::open(tmp.path()).unwrap();
    symlink(outside.path(), tmp.path().join(".jmcl")).unwrap();
    assert!(install::prepare(&root, &plan()).is_err());
    assert!(root.atomic_replace(".jmcl/escape", b"not written").is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    root.remove_tree(".jmcl").unwrap();
    assert!(outside.path().is_dir());
}

fn zip64_archive() -> Vec<u8> {
    let name = b"native.bin";
    let mut local = vec![0; 30];
    local[..4].copy_from_slice(b"PK\x03\x04");
    local[4..6].copy_from_slice(&45u16.to_le_bytes());
    local[18..26].fill(255);
    local[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
    local[28..30].copy_from_slice(&20u16.to_le_bytes());
    local.extend(name);
    local.extend(1u16.to_le_bytes());
    local.extend(16u16.to_le_bytes());
    local.extend(3u64.to_le_bytes());
    local.extend(3u64.to_le_bytes());
    local.extend(b"abc");
    let offset = local.len() as u64;
    let mut central = vec![0; 46];
    central[..4].copy_from_slice(b"PK\x01\x02");
    central[4..6].copy_from_slice(&0x314u16.to_le_bytes());
    central[6..8].copy_from_slice(&45u16.to_le_bytes());
    central[20..28].fill(255);
    central[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
    central[30..32].copy_from_slice(&28u16.to_le_bytes());
    central[38..42].copy_from_slice(&(0o100644u32 << 16).to_le_bytes());
    central[42..46].fill(255);
    central.extend(name);
    central.extend(1u16.to_le_bytes());
    central.extend(24u16.to_le_bytes());
    central.extend(3u64.to_le_bytes());
    central.extend(3u64.to_le_bytes());
    central.extend(0u64.to_le_bytes());
    let size = central.len() as u64;
    local.extend(central);
    let record_offset = local.len() as u64;
    let mut end64 = vec![0; 56];
    end64[..4].copy_from_slice(b"PK\x06\x06");
    end64[4..12].copy_from_slice(&44u64.to_le_bytes());
    end64[12..14].copy_from_slice(&45u16.to_le_bytes());
    end64[14..16].copy_from_slice(&45u16.to_le_bytes());
    end64[24..32].copy_from_slice(&1u64.to_le_bytes());
    end64[32..40].copy_from_slice(&1u64.to_le_bytes());
    end64[40..48].copy_from_slice(&size.to_le_bytes());
    end64[48..56].copy_from_slice(&offset.to_le_bytes());
    local.extend(end64);
    let mut locator = vec![0; 20];
    locator[..4].copy_from_slice(b"PK\x06\x07");
    locator[8..16].copy_from_slice(&record_offset.to_le_bytes());
    locator[16..20].copy_from_slice(&1u32.to_le_bytes());
    local.extend(locator);
    let mut end = vec![0; 22];
    end[..4].copy_from_slice(b"PK\x05\x06");
    end[8..20].fill(255);
    local.extend(end);
    local
}
#[test]
fn zip64_native_entry_and_directory_extents_are_supported() {
    let tmp = tempfile::tempdir().unwrap();
    let root = ArtifactRoot::open(tmp.path()).unwrap();
    let b = zip64_archive();
    root.atomic_replace("native.jar", &b).unwrap();
    let mut p = plan();
    p.files.push(planned("native.jar", &b));
    p.native_extractions.push(NativeExtraction {
        archive_path: "native.jar".into(),
        exclude: vec![],
    });
    let result = install::prepare(&root, &p).unwrap();
    let path = result.native_directory.unwrap();
    assert_eq!(
        fs::read(tmp.path().join(path).join("native.bin")).unwrap(),
        b"abc"
    );
}
#[test]
#[ignore = "requires pinned Zig preparer fixture adapter"]
fn zip64_and_entry_count_match_pinned_zig() {
    let oracle = std::env::var("JMCL_ZIG_PREPARE_ORACLE").unwrap();
    // Keep the original truncated fixture; vary only its declared filename
    // length beyond Darwin/Linux PATH_MAX but below the larger Windows bound.
    let long_name = |length: u16| {
        let mut bytes = archive(&[entry("native.bin")]);
        let central = bytes.windows(4).position(|v| v == b"PK\x01\x02").unwrap();
        bytes[central + 28..central + 30].copy_from_slice(&length.to_le_bytes());
        bytes
    };
    for (name, b) in [
        ("name-1025-before-truncated", long_name(1025)),
        ("name-4097-before-truncated", long_name(4097)),
        ("name-8192-before-truncated", long_name(8192)),
        ("zip64", zip64_archive()),
        ("entry-limit", archive(&vec![entry("same"); 4097])),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let rust = tmp.path().join("rust");
        let zig = tmp.path().join("zig");
        let root = ArtifactRoot::open(&rust).unwrap();
        fs::create_dir(&zig).unwrap();
        root.atomic_replace("native.jar", &b).unwrap();
        fs::write(zig.join("native.jar"), &b).unwrap();
        let mut p = plan();
        p.files.push(planned("native.jar", &b));
        p.native_extractions.push(NativeExtraction {
            archive_path: "native.jar".into(),
            exclude: vec!["same".into()],
        });
        let fixture = tmp.path().join("plan.json");
        fs::write(&fixture, serde_json::to_vec(&p).unwrap()).unwrap();
        let out = Command::new(&oracle)
            .arg(fixture)
            .arg(&zig)
            .output()
            .unwrap();
        assert!(out.status.success());
        let mut zv: Value = serde_json::from_slice(&out.stdout).unwrap();
        let mut rv = run_rust(&root, &p);
        normalize(&mut zv, &zig);
        normalize(&mut rv, &rust);
        assert_eq!(rv, zv, "{name}");
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires pinned Zig preparer fixture adapter"]
fn raw_unix_native_names_match_without_lossy_writes() {
    use std::os::unix::ffi::OsStrExt;
    let tmp = tempfile::tempdir().unwrap();
    let rust = tmp.path().join("rust");
    let zig = tmp.path().join("zig");
    let root = ArtifactRoot::open(&rust).unwrap();
    fs::create_dir(&zig).unwrap();
    let mut b = archive(&[entry("raw.bin")]);
    for i in 0..b.len() - 6 {
        if &b[i..i + 7] == b"raw.bin" {
            b[i] = 0xff;
        }
    }
    root.atomic_replace("native.jar", &b).unwrap();
    fs::write(zig.join("native.jar"), &b).unwrap();
    let mut p = plan();
    p.files.push(planned("native.jar", &b));
    p.native_extractions.push(NativeExtraction {
        archive_path: "native.jar".into(),
        exclude: vec![],
    });
    let fixture = tmp.path().join("plan.json");
    fs::write(&fixture, serde_json::to_vec(&p).unwrap()).unwrap();
    let out = Command::new(std::env::var("JMCL_ZIG_PREPARE_ORACLE").unwrap())
        .arg(fixture)
        .arg(&zig)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let zv: Value = serde_json::from_slice(&out.stdout).unwrap();
    let support = raw_names::probe(&rust, raw_names::Kind::File);
    let rv = install::prepare(&root, &p);
    if let raw_names::Support::Rejected(error) = support {
        let rust_error = rv.expect_err("raw native filename must be rejected by this filesystem");
        let zig_error = zv["error"]
            .as_str()
            .expect("Zig must also reject the raw native filename");
        assert_eq!(
            serde_json::json!({"error": rust_error.name()}),
            zv,
            "filesystem rejection must retain exact error parity; a mismatch is a production contract gap, not an unsupported-success skip"
        );
        eprintln!(
            "raw-native success unsupported on fixture filesystem ({error}); both cores reject identically and clean up (Zig={zig_error}, Rust={})",
            rust_error.name()
        );
        for directory in [&rust, &zig] {
            assert_eq!(fs::read(directory.join("native.jar")).unwrap(), b);
            let native_parent = directory.join(".jmcl/runtime/natives");
            match fs::read_dir(native_parent) {
                Ok(entries) => {
                    assert_eq!(entries.count(), 0, "failed extraction left a native tree")
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => panic!("cannot verify native cleanup: {e}"),
            }
        }
        return;
    }
    let rv = rv.unwrap();
    assert_eq!(zv["native_files"], 1);
    assert_eq!(rv.native_files, 1);
    let name = std::ffi::OsStr::from_bytes(b"\xffaw.bin");
    assert_eq!(
        fs::read(
            zig.join(zv["native_directory"].as_str().unwrap())
                .join(name)
        )
        .unwrap(),
        b"abc"
    );
    assert_eq!(
        fs::read(rust.join(rv.native_directory.as_ref().unwrap()).join(name)).unwrap(),
        b"abc"
    );
    install::cleanup_native(&root, rv.native_directory.as_ref().unwrap()).unwrap();
}
#[test]
fn launch_paths_derive_from_held_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = ArtifactRoot::open(tmp.path().join("root")).unwrap();
    root.ensure_directory("native").unwrap();
    assert_eq!(
        root.absolute_path().unwrap(),
        fs::canonicalize(tmp.path().join("root")).unwrap()
    );
    assert_eq!(
        root.absolute_directory("native").unwrap(),
        fs::canonicalize(tmp.path().join("root/native")).unwrap()
    );
    #[cfg(unix)]
    {
        fs::rename(tmp.path().join("root"), tmp.path().join("renamed")).unwrap();
        fs::create_dir(tmp.path().join("root")).unwrap();
        assert_eq!(
            root.absolute_path().unwrap(),
            fs::canonicalize(tmp.path().join("renamed")).unwrap()
        );
    }
}

#[test]
fn byte_path_replacement_remains_atomic_and_anchored() {
    let tmp = tempfile::tempdir().unwrap();
    let root = ArtifactRoot::open(tmp.path()).unwrap();
    root.atomic_replace_bytes(b"config/name", b"one").unwrap();
    root.atomic_replace_bytes(b"config/name", b"two").unwrap();
    assert_eq!(fs::read(tmp.path().join("config/name")).unwrap(), b"two");
    assert!(root.atomic_replace_bytes(b"../outside", b"no").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        match raw_names::probe(&tmp.path().join("config"), raw_names::Kind::File) {
            raw_names::Support::Supported => {
                root.atomic_replace_bytes(b"config/\xff", b"raw").unwrap();
                assert_eq!(
                    fs::read(tmp.path().join(std::ffi::OsStr::from_bytes(b"config/\xff"))).unwrap(),
                    b"raw"
                );
            }
            raw_names::Support::Rejected(probe_error) => {
                let error = root
                    .atomic_replace_bytes(b"config/\xff", b"raw")
                    .expect_err("raw filename replacement must be rejected on this filesystem");
                assert_eq!(error.raw_os_error(), probe_error.raw_os_error());
                assert_eq!(fs::read(tmp.path().join("config/name")).unwrap(), b"two");
                let names = fs::read_dir(tmp.path().join("config"))
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<Vec<_>>();
                assert_eq!(
                    names,
                    vec![std::ffi::OsString::from("name")],
                    "failed replacement leaked temporary files"
                );
                eprintln!(
                    "raw-byte replacement success unsupported on fixture filesystem ({probe_error}); rejection and atomic cleanup verified instead"
                );
            }
        }
    }
}

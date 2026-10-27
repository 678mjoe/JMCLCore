use super::projection_tests::{plan, snapshot, source};
use super::*;
use crate::{install, store::ArtifactRoot};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::{Cursor, Write},
    os::unix::{
        ffi::OsStringExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::Path,
};
// Exactly two regular stored files, no implicit directories or external inputs.
fn archive() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        ("data/plain.txt", b"inert native fixture".as_slice()),
        ("META-INF/note.txt", b"excluded note".as_slice()),
    ] {
        assert!(name.is_ascii() && !name.contains("..") && !name.starts_with('/'));
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored)
                    .unix_permissions(0o644),
            )
            .unwrap();
        writer.write_all(body).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn native(id: serde_json::Value, bytes: &[u8]) -> ReadPlan {
    let mut p = plan(id);
    p.files.push(ReadFile {
        path: PlanBytes(b"libraries/fixture.jar".to_vec()),
        url: "https://example.invalid/unused".into(),
        fallback_url: None,
        sha1: format!("{:x}", Sha1::digest(bytes)),
        size: bytes.len() as u64,
        usage: Usage {
            native: true,
            ..Default::default()
        },
    });
    p.native_extractions.push(NativeExtraction {
        archive_path: "libraries/fixture.jar".into(),
        exclude: vec!["META-INF/".into()],
    });
    p
}
fn write_archive(root: &Path, bytes: &[u8]) -> std::path::PathBuf {
    let path = root.join("libraries/fixture.jar");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    path
}
fn nonce(out: &mut [u8; 8]) -> Result<(), install::Error> {
    *out = 77u64.to_ne_bytes();
    Ok(())
}
#[test]
fn owned_benign_native_equivalence_opaque_identity_exclusion_and_peers() {
    let bytes = archive();
    assert!(bytes.len() < 1024);
    for id in [serde_json::json!("normal"), serde_json::json!([255])] {
        let p = native(id, &bytes);
        assert!(p.zero_native_preparation().is_none());
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"artifact-\xff".to_vec()));
        assert!(path.is_absolute());
        let root = ArtifactRoot::open(&path).unwrap();
        source(&path, &p);
        let archive_path = write_archive(&path, &bytes);
        let peer = temp.path().join("readonly-cas-peer");
        fs::hard_link(&archive_path, &peer).unwrap();
        fs::write(temp.path().join("sibling"), b"canary").unwrap();
        let before = snapshot(temp.path());
        let result = p.prepare_game_native_with(&root, nonce).unwrap();
        assert_eq!(
            (
                result.native_files,
                result.projections_created,
                result.projections_cached
            ),
            (1, 2, 0)
        );
        assert_eq!(
            result.native_directory.as_deref(),
            Some(".jmcl/runtime/natives/4d")
        );
        let directory = path.join(result.native_directory.as_ref().unwrap());
        assert_eq!(
            fs::read(directory.join("data/plain.txt")).unwrap(),
            b"inert native fixture"
        );
        assert!(!directory.join("META-INF").exists());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert_eq!(fs::read_dir(directory.join("data")).unwrap().count(), 1);
        assert_eq!(
            fs::metadata(directory.join("data/plain.txt"))
                .unwrap()
                .mode()
                & 0o111,
            0
        );
        let after = snapshot(temp.path());
        for name in [
            std::path::Path::new("readonly-cas-peer"),
            std::path::Path::new("sibling"),
        ] {
            assert_eq!(before[name], after[name]);
        }
        let rel = archive_path.strip_prefix(temp.path()).unwrap();
        assert_eq!(before[rel], after[rel]);
        let sp = native(serde_json::json!("normal"), &bytes).into_string();
        let other = temp.path().join("string");
        let sr = ArtifactRoot::open(&other).unwrap();
        source(&other, &p);
        write_archive(&other, &bytes);
        let baseline = install::prepare::prepare_with(
            &sr,
            &sp.asset_projections,
            &sp.native_extractions,
            |name| {
                sp.files
                    .iter()
                    .find(|f| f.path == name)
                    .map(|f| (f.sha1.as_str(), f.size))
            },
            nonce,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&result).unwrap(),
            serde_json::to_vec(&baseline).unwrap()
        );
        assert_eq!(
            fs::read(other.join(".jmcl/runtime/natives/4d/data/plain.txt")).unwrap(),
            b"inert native fixture"
        );
    }
}
#[test]
fn owned_native_preflight_missing_size_hash_failures_precede_directory_creation() {
    let bytes = archive();
    for failure in ["missing", "size", "hash"] {
        let mut p = native(serde_json::json!([255]), &bytes);
        p.asset_projections.clear();
        if failure == "size" {
            p.files.last_mut().unwrap().size += 1;
        } else if failure == "hash" {
            p.files.last_mut().unwrap().sha1 = "0".repeat(40);
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("artifact");
        assert!(path.is_absolute());
        let root = ArtifactRoot::open(&path).unwrap();
        if failure != "missing" {
            let a = write_archive(&path, &bytes);
            fs::hard_link(a, temp.path().join("peer")).unwrap();
        }
        fs::write(temp.path().join("sibling"), b"canary").unwrap();
        let before = snapshot(temp.path());
        assert_eq!(
            p.prepare_game_native(&root).unwrap_err().name(),
            if failure == "missing" {
                "FileNotFound"
            } else {
                "CacheFileInvalid"
            }
        );
        assert_eq!(snapshot(temp.path()), before);
        assert!(!path.join(".jmcl").exists());
    }
}
#[test]
fn owned_projection_failure_precedes_native_lookup_and_native_failure_keeps_projections() {
    let bytes = archive();
    let p = native(serde_json::json!([255]), &bytes);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("artifact");
    assert!(path.is_absolute());
    let root = ArtifactRoot::open(&path).unwrap();
    let before = snapshot(temp.path());
    let mut calls = 0;
    assert_eq!(
        p.prepare_game_native_with(&root, |_| {
            calls += 1;
            panic!("nonce must not run")
        })
        .unwrap_err()
        .name(),
        "FileNotFound"
    );
    assert_eq!(calls, 0);
    assert_eq!(snapshot(temp.path()), before);
    let src = source(&path, &p);
    fs::hard_link(&src, temp.path().join("peer")).unwrap();
    assert_eq!(
        p.prepare_game_native(&root).unwrap_err().name(),
        "FileNotFound"
    );
    for projection in &p.asset_projections {
        assert_eq!(
            fs::read(path.join(&projection.destination_path)).unwrap(),
            fs::read(&src).unwrap()
        );
        assert_eq!(
            fs::metadata(path.join(&projection.destination_path))
                .unwrap()
                .ino(),
            fs::metadata(&src).unwrap().ino()
        );
    }
    assert_eq!(fs::metadata(&src).unwrap().nlink(), 4);
    assert_eq!(fs::metadata(&src).unwrap().mode() & 0o222, 0);
    assert!(!path.join(".jmcl").exists());
    assert!(!path.join("libraries").exists());
}

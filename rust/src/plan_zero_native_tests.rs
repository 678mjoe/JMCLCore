use super::projection_tests::{plan, snapshot, source};
use super::*;
use crate::{
    install::{self, Error},
    store::ArtifactRoot,
};
use std::{fs, os::unix::ffi::OsStringExt};
fn nonce(out: &mut [u8; 8]) -> Result<(), Error> {
    *out = 42u64.to_ne_bytes();
    Ok(())
}
#[test]
fn capability_excludes_native_descriptions_before_root_construction() {
    let mut p = plan(serde_json::json!([255]));
    p.native_extractions.push(NativeExtraction {
        archive_path: "never-opened.jar".into(),
        exclude: vec![],
    });
    let mut called = false;
    if let Some(cap) = p.zero_native_preparation() {
        called = true;
        let _ = cap;
    }
    assert!(!called);
}
#[test]
fn owned_empty_and_projected_byte_roots_match_string_phase_and_preserve_peers() {
    for projected in [false, true] {
        let mut p = plan(serde_json::json!([255]));
        if !projected {
            p.asset_projections.clear();
        }
        let cap = p.zero_native_preparation().unwrap(); // Before fixture construction.
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join(std::ffi::OsString::from_vec(b"artifact-\xff".to_vec()));
        assert!(path.is_absolute());
        let root = ArtifactRoot::open(&path).unwrap();
        fs::write(temp.path().join("sibling"), b"canary").unwrap();
        if projected {
            let s = source(&path, &p);
            fs::hard_link(s, temp.path().join("peer")).unwrap();
        }
        let result = cap.prepare_with(&root, nonce).unwrap();
        assert_eq!(
            result.native_directory.as_deref(),
            Some(".jmcl/runtime/natives/2a")
        );
        assert_eq!(result.native_files, 0);
        assert_eq!(result.projections_created, if projected { 2 } else { 0 });
        assert_eq!(
            fs::read_dir(path.join(".jmcl/runtime/natives/2a"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(path.join(".jmcl/runtime/natives"))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(fs::read(temp.path().join("sibling")).unwrap(), b"canary");
        let mut ordinary = plan(serde_json::json!("normal"));
        if !projected {
            ordinary.asset_projections.clear();
        }
        let ordinary = ordinary.into_string();
        let other = temp.path().join("string");
        let sr = ArtifactRoot::open(&other).unwrap();
        if projected {
            source(&other, &p);
        }
        let mut baseline = install::Preparation {
            native_directory: None,
            native_files: 0,
            projections_created: 0,
            projections_cached: 0,
        };
        install::prepare::project_only(
            &sr,
            &ordinary.asset_projections,
            |path| {
                ordinary
                    .files
                    .iter()
                    .find(|f| f.path == path)
                    .map(|f| (f.sha1.as_str(), f.size))
            },
            &mut baseline,
        )
        .unwrap();
        let (name, handle) = install::prepare::create_native_directory(&sr, nonce).unwrap();
        drop(handle);
        baseline.native_directory = Some(name);
        assert_eq!(
            serde_json::to_vec(&result).unwrap(),
            serde_json::to_vec(&baseline).unwrap()
        );
        let before = snapshot(&path);
        let repeated = cap
            .prepare_with(&root, |b| {
                *b = 43u64.to_ne_bytes();
                Ok(())
            })
            .unwrap();
        assert_eq!(repeated.projections_cached, if projected { 2 } else { 0 });
        let after = snapshot(&path);
        for (path, old) in before {
            let current = &after[&path];
            assert_eq!(
                (&current.0, current.1, current.2, current.3),
                (&old.0, old.1, old.2, old.3)
            );
            if !path.as_os_str().is_empty() && path != std::path::Path::new(".jmcl/runtime/natives")
            {
                assert_eq!(current.4, old.4);
            }
        }
    }
}
#[test]
fn owned_collision_retry_and_exhaustion_preserve_existing_directories() {
    let mut p = plan(serde_json::json!([255]));
    p.asset_projections.clear();
    let cap = p.zero_native_preparation().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("artifact");
    assert!(path.is_absolute());
    let root = ArtifactRoot::open(&path).unwrap();
    cap.prepare_with(&root, nonce).unwrap();
    let before = snapshot(temp.path());
    let mut calls = 0;
    assert_eq!(
        cap.prepare_with(&root, |b| {
            calls += 1;
            nonce(b)
        })
        .unwrap_err()
        .name(),
        "NativeDirectoryCollision"
    );
    assert_eq!(calls, 16);
    assert_eq!(snapshot(temp.path()), before);
    calls = 0;
    let result = cap
        .prepare_with(&root, |b| {
            calls += 1;
            *b = if calls == 1 { 42u64 } else { 44u64 }.to_ne_bytes();
            Ok(())
        })
        .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(
        result.native_directory.as_deref(),
        Some(".jmcl/runtime/natives/2c")
    );
}
#[test]
fn owned_projection_and_parent_failures_precede_rng_and_retain_required_parents() {
    let mut p = plan(serde_json::json!([255]));
    assert!(p.zero_native_preparation().is_some());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("artifact");
    assert!(path.is_absolute());
    let root = ArtifactRoot::open(&path).unwrap();
    fs::write(temp.path().join("sibling"), b"canary").unwrap();
    fs::hard_link(temp.path().join("sibling"), temp.path().join("peer")).unwrap();
    let mut calls = 0;
    let before = snapshot(temp.path());
    assert_eq!(
        p.zero_native_preparation()
            .unwrap()
            .prepare_with(&root, |_| {
                calls += 1;
                Err(Error::named("RandomSourceFailed"))
            })
            .unwrap_err()
            .name(),
        "FileNotFound"
    );
    assert_eq!(calls, 0);
    assert_eq!(snapshot(temp.path()), before);
    p.asset_projections.clear();
    let cap = p.zero_native_preparation().unwrap();
    assert_eq!(
        cap.prepare_with(&root, |_| {
            calls += 1;
            assert!(path.join(".jmcl/runtime/natives").is_dir());
            assert_eq!(
                fs::read_dir(path.join(".jmcl/runtime/natives"))
                    .unwrap()
                    .count(),
                0
            );
            Err(Error::named("RandomSourceFailed"))
        })
        .unwrap_err()
        .name(),
        "RandomSourceFailed"
    );
    assert_eq!(calls, 1);
    let after = snapshot(temp.path());
    let added: Vec<_> = after
        .keys()
        .filter(|k| !before.contains_key(*k))
        .map(|p| p.to_str().unwrap())
        .collect();
    assert_eq!(
        added,
        [
            "artifact/.jmcl",
            "artifact/.jmcl/runtime",
            "artifact/.jmcl/runtime/natives"
        ]
    );
    for name in ["sibling", "peer"] {
        assert_eq!(
            after[std::path::Path::new(name)],
            before[std::path::Path::new(name)]
        );
    }
    let blocked = temp.path().join("blocked-artifact");
    assert!(blocked.is_absolute());
    let blocked_root = ArtifactRoot::open(&blocked).unwrap();
    fs::write(blocked.join(".jmcl"), b"blocked").unwrap();
    let before = snapshot(temp.path());
    calls = 0;
    assert_eq!(
        cap.prepare_with(&blocked_root, |_| {
            calls += 1;
            Err(Error::named("RandomSourceFailed"))
        })
        .unwrap_err()
        .name(),
        "FileIoError"
    );
    assert_eq!(calls, 0);
    assert_eq!(snapshot(temp.path()), before);
    assert_eq!(
        cap.prepare(&blocked_root).unwrap_err().name(),
        "FileIoError"
    );
    assert_eq!(snapshot(temp.path()), before);
}

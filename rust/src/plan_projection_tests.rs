use super::*;
use crate::{metadata, store::ArtifactRoot};
use sha1::{Digest, Sha1};
use std::{
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

type Snapshot = std::collections::BTreeMap<std::path::PathBuf, (Vec<u8>, u64, u64, u32, u64)>;
fn snapshot(root: &Path) -> Snapshot {
    fn walk(root: &Path, path: &Path, out: &mut Snapshot) {
        let m = fs::symlink_metadata(path).unwrap();
        out.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            (
                if m.is_file() {
                    fs::read(path).unwrap()
                } else {
                    vec![]
                },
                m.dev(),
                m.ino(),
                m.mode(),
                m.nlink(),
            ),
        );
        if m.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), out)
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

const BODY: &[u8] = b"opaque synthetic projection bytes";
fn plan(id: serde_json::Value) -> ReadPlan {
    let h = format!("{:x}", Sha1::digest(BODY));
    let v = serde_json::json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":"https://example.invalid/client","sha1":"a".repeat(40),"size":1}},"assetIndex":{"id":"idx","url":"https://example.invalid/index","sha1":"b".repeat(40),"size":2,"totalSize":BODY.len()}});
    let m = metadata::ReadMetadata::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
    let a = metadata::asset_index(&serde_json::to_vec(&serde_json::json!({"objects":{"sound/a":{"hash":h,"size":BODY.len()}},"virtual":true,"map_to_resources":true})).unwrap()).unwrap();
    build_read(
        &m,
        Some(&a),
        Source::Official,
        Target {
            os: "linux".into(),
            arch: "x86_64".into(),
            os_version: None,
        },
    )
    .unwrap()
}
fn root(p: &Path) -> ArtifactRoot {
    assert!(p.is_absolute() && p.file_name().unwrap() == "artifact");
    ArtifactRoot::open(p).unwrap()
}
fn identity(p: &Path) -> (Vec<u8>, u64, u64, u32, u64) {
    let m = fs::metadata(p).unwrap();
    (fs::read(p).unwrap(), m.dev(), m.ino(), m.mode(), m.nlink())
}
fn source(p: &Path, plan: &ReadPlan) -> std::path::PathBuf {
    let s = p.join(&plan.asset_projections[0].source_path);
    fs::create_dir_all(s.parent().unwrap()).unwrap();
    fs::write(&s, BODY).unwrap();
    fs::set_permissions(&s, fs::Permissions::from_mode(0o444)).unwrap();
    s
}
#[test]
fn owned_create_repeat_and_string_equivalence_preserve_peers() {
    for id in [serde_json::json!("normal"), serde_json::json!([255])] {
        let temp = tempfile::tempdir().unwrap();
        let p = temp.path().join("artifact");
        let r = root(&p);
        let plan = plan(id);
        let s = source(&p, &plan);
        let peer = temp.path().join("peer");
        fs::hard_link(&s, &peer).unwrap();
        let sibling = temp.path().join("sibling");
        fs::write(&sibling, b"canary").unwrap();
        let before = identity(&peer);
        let locks = p.join("existing-locks");
        fs::create_dir(&locks).unwrap();
        fs::write(locks.join("digest.lock"), b"visible").unwrap();
        let result = plan.project_assets(&r).unwrap();
        assert_eq!(
            (result.projections_created, result.projections_cached),
            (2, 0)
        );
        assert_eq!(result.native_directory, None);
        for x in &plan.asset_projections {
            let d = identity(&p.join(&x.destination_path));
            assert_eq!(
                (&d.0, d.1, d.2, d.3),
                (&before.0, before.1, before.2, before.3)
            );
            assert_eq!(d.4, 4);
        }
        let snapshot_before = snapshot(temp.path());
        let result = plan.project_assets(&r).unwrap();
        assert_eq!(
            (result.projections_created, result.projections_cached),
            (0, 2)
        );
        assert_eq!(snapshot(temp.path()), snapshot_before);
        assert_eq!(identity(&peer).4, 4);
        let string = self::plan(serde_json::json!("normal")).into_string();
        let mut counts = crate::install::Preparation {
            native_directory: None,
            native_files: 0,
            projections_created: 0,
            projections_cached: 0,
        };
        crate::install::prepare::project_only(
            &r,
            &string.asset_projections,
            |path| {
                string
                    .files
                    .iter()
                    .find(|f| f.path == path)
                    .map(|f| (f.sha1.as_str(), f.size))
            },
            &mut counts,
        )
        .unwrap();
        assert_eq!(counts, result);
        assert_eq!(fs::read(sibling).unwrap(), b"canary");
        assert_eq!(fs::read(locks.join("digest.lock")).unwrap(), b"visible");
        assert!(!p.join(".jmcl").exists());
    }
}
#[test]
fn owned_replacement_preserves_old_readonly_inode_and_cleans_temporary_files() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("artifact");
    let r = root(&p);
    let plan = plan(serde_json::json!([255]));
    source(&p, &plan);
    let dest = p.join(&plan.asset_projections[0].destination_path);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, b"old").unwrap();
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o444)).unwrap();
    let peer = temp.path().join("peer");
    fs::hard_link(&dest, &peer).unwrap();
    let before = identity(&peer);
    assert_eq!(plan.project_assets(&r).unwrap().projections_created, 2);
    let after = identity(&peer);
    assert_eq!(
        (&after.0, after.1, after.2, after.3),
        (&before.0, before.1, before.2, before.3)
    );
    assert_eq!(after.4, 1);
    assert_eq!(fs::read(&dest).unwrap(), BODY);
    assert_ne!(identity(&dest).2, before.2);
    // The unchanged writer requests 0644; the process umask still applies.
    let mode_control = temp.path().join("mode-control");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&mode_control)
        .unwrap();
    assert_eq!(identity(&dest).3 & 0o777, identity(&mode_control).3 & 0o777);
    assert_eq!(fs::read_dir(dest.parent().unwrap()).unwrap().count(), 1);
    assert!(!p.join(".jmcl").exists());
}
#[test]
fn owned_missing_corrupt_and_blocked_parent_preserve_error_order() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("artifact");
    let r = root(&p);
    let mut plan = plan(serde_json::json!([255]));
    let before = snapshot(temp.path());
    let missing = plan.project_assets(&r).unwrap_err();
    assert_eq!(snapshot(temp.path()), before);
    assert_eq!(missing.name(), "FileNotFound");
    assert!(!p.join("assets/virtual").exists());
    let s = source(&p, &plan);
    fs::set_permissions(&s, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&s, b"bad").unwrap();
    assert_eq!(
        plan.project_assets(&r).unwrap_err().name(),
        "CacheFileInvalid"
    );
    assert!(!p.join("assets/virtual").exists());
    fs::write(&s, vec![0; BODY.len()]).unwrap();
    let before = snapshot(temp.path());
    assert_eq!(
        plan.project_assets(&r).unwrap_err().name(),
        "CacheFileInvalid"
    );
    assert_eq!(snapshot(temp.path()), before);
    fs::write(&s, BODY).unwrap();
    fs::write(p.join("assets/virtual"), b"blocked").unwrap();
    let before = snapshot(temp.path());
    assert_eq!(plan.project_assets(&r).unwrap_err().name(), "FileIoError");
    assert_eq!(fs::read(p.join("assets/virtual")).unwrap(), b"blocked");
    assert_eq!(fs::read(s).unwrap(), BODY);
    assert_eq!(snapshot(temp.path()), before);
    plan.files
        .retain(|f| f.path.0 != plan.asset_projections[0].source_path.as_bytes());
    assert_eq!(
        plan.project_assets(&r).unwrap_err().name(),
        "ProjectionSourceNotPlanned"
    );
    assert!(!p.join("resources").exists());
    assert!(!p.join(".jmcl").exists());
}

#[test]
fn owned_empty_projection_view_has_no_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("artifact");
    let r = root(&p);
    let mut plan = plan(serde_json::json!([255]));
    plan.asset_projections.clear();
    let before = snapshot(temp.path());
    let result = plan.project_assets(&r).unwrap();
    assert_eq!(
        (
            result.projections_created,
            result.projections_cached,
            result.native_files
        ),
        (0, 0, 0)
    );
    assert_eq!(result.native_directory, None);
    assert_eq!(snapshot(temp.path()), before);
}

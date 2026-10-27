use super::*;
fn target() -> Target {
    Target {
        os: "linux".into(),
        arch: "x86_64".into(),
        os_version: Some("not serialized".into()),
    }
}
fn identity(version: &[u8]) -> Identity<'_> {
    Identity {
        version,
        fabric: None,
        neo: None,
        forge: None,
        source: Source::Official,
    }
}
#[test]
fn codec_preserves_string_field_order_newline_and_opaque_identity() {
    let p = identity("版本".as_bytes());
    assert_eq!(encode(&p,&target()).unwrap(),"{\"schema_version\":1,\"version_id\":\"版本\",\"fabric_loader\":null,\"neoforge_version\":null,\"forge_version\":null,\"source\":\"official\",\"os\":\"linux\",\"arch\":\"x86_64\"}\n".as_bytes());
    let mut p = identity(&[255, 0]);
    p.fabric = Some(&[254]);
    p.source = Source::Bmclapi;
    assert_eq!(encode(&p,&target()).unwrap(),b"{\"schema_version\":1,\"version_id\":[255,0],\"fabric_loader\":[254],\"neoforge_version\":null,\"forge_version\":null,\"source\":\"bmclapi\",\"os\":\"linux\",\"arch\":\"x86_64\"}\n");
}
#[test]
fn receipt_validation_keeps_version_loader_conflict_target_order() {
    let mut p = identity(b"");
    p.fabric = Some(b"");
    p.neo = Some(b"x");
    let mut t = target();
    t.os.clear();
    assert_eq!(encode(&p, &t).unwrap_err().name(), "InvalidVersionId");
    p.version = b"v";
    assert_eq!(encode(&p, &t).unwrap_err().name(), "InvalidLoaderVersion");
    p.fabric = Some(b"a");
    assert_eq!(encode(&p, &t).unwrap_err().name(), "LoaderConflict");
    p.neo = None;
    assert_eq!(
        encode(&p, &t).unwrap_err().name(),
        "InvalidInstallationTarget"
    );
    let long = vec![255; 257];
    assert_eq!(
        encode(&identity(&long), &target()).unwrap_err().name(),
        "InvalidVersionId"
    );
    let max = vec![255; 256];
    assert!(encode(&identity(&max), &target()).is_ok());
}
#[cfg(target_os = "linux")]
mod owned {
    use super::*;
    use std::{
        collections::BTreeMap,
        fs,
        os::unix::fs::MetadataExt,
        path::{Path, PathBuf},
    };
    type Snap = (Vec<u8>, u64, u64, u64, u32, i64, i64);
    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Snap> {
        fn walk(root: &Path, p: &Path, o: &mut BTreeMap<PathBuf, Snap>) {
            let m = fs::symlink_metadata(p).unwrap();
            o.insert(
                p.strip_prefix(root).unwrap().into(),
                (
                    if m.is_file() {
                        fs::read(p).unwrap()
                    } else {
                        vec![]
                    },
                    m.dev(),
                    m.ino(),
                    m.nlink(),
                    m.mode(),
                    m.mtime(),
                    m.mtime_nsec(),
                ),
            );
            if m.is_dir() {
                for e in fs::read_dir(p).unwrap() {
                    walk(root, &e.unwrap().path(), o)
                }
            }
        }
        let mut o = BTreeMap::new();
        walk(root, root, &mut o);
        o
    }
    struct Owned {
        owner: tempfile::TempDir,
        root: ArtifactRoot,
        path: PathBuf,
        canaries: BTreeMap<PathBuf, Snap>,
    }
    fn guarded(owner: &Path, instances: &Path, id: &str) -> bool {
        !instances.as_os_str().is_empty()
            && instances == owner.join("instances")
            && id == "demo"
            && crate::instances::validate_id(id).is_ok()
    }
    impl Owned {
        fn new() -> Self {
            let owner = tempfile::tempdir().unwrap();
            let instances = owner.path().join("instances");
            assert!(guarded(owner.path(), &instances, "demo"));
            let path = instances.join("demo/.minecraft");
            let root = ArtifactRoot::open(&path).unwrap();
            let sibling = owner.path().join("sibling");
            fs::create_dir(&sibling).unwrap();
            fs::write(sibling.join("sentinel"), b"unchanged").unwrap();
            fs::hard_link(sibling.join("sentinel"), sibling.join("peer")).unwrap();
            let canaries = snapshot(&sibling);
            Self {
                owner,
                root,
                path,
                canaries,
            }
        }
        fn check(&self) {
            assert_eq!(snapshot(&self.owner.path().join("sibling")), self.canaries);
        }
    }
    #[test]
    fn driver_guards_reject_ambient_roots_and_invalid_instance_ids() {
        let owner = Path::new("/owned-placeholder");
        assert!(!guarded(owner, Path::new(""), "demo"));
        assert!(!guarded(owner, Path::new("instances"), "demo"));
        assert!(!guarded(owner, &owner.join("instances"), "../bad"));
        assert!(!guarded(owner, &owner.join("instances"), "other"));
    }
    #[test]
    fn writer_matches_public_strings_and_replaces_readonly_hardlink_without_truncation() {
        let o = Owned::new();
        let p = identity("版本".as_bytes());
        let public = crate::install::service::Selection {
            id: "版本".into(),
            fabric_loader: None,
            neoforge_version: None,
            forge_version: None,
            source: Source::Official,
            os: None,
            arch: None,
        };
        crate::install::service::record_completed(&o.root, &public, &target()).unwrap();
        let receipt = o.path.join(".jmcl/install.json");
        assert_eq!(fs::read(&receipt).unwrap(), encode(&p, &target()).unwrap());
        let peer = o.owner.path().join("receipt-peer");
        fs::hard_link(&receipt, &peer).unwrap();
        let mut perms = fs::metadata(&peer).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&peer, perms).unwrap();
        let old = fs::metadata(&peer).unwrap();
        let old_bytes = fs::read(&peer).unwrap();
        assert_eq!(old.nlink(), 2);
        write(&o.root, &identity(&[255]), &target()).unwrap();
        assert_eq!(
            fs::read(&receipt).unwrap(),
            encode(&identity(&[255]), &target()).unwrap()
        );
        let now = fs::metadata(&peer).unwrap();
        let new = fs::metadata(&receipt).unwrap();
        assert_eq!(fs::read(&peer).unwrap(), old_bytes);
        assert_eq!(
            (
                old.dev(),
                old.ino(),
                old.mode(),
                old.mtime(),
                old.mtime_nsec()
            ),
            (
                now.dev(),
                now.ino(),
                now.mode(),
                now.mtime(),
                now.mtime_nsec()
            )
        );
        assert_eq!(now.nlink(), 1);
        assert_eq!(new.nlink(), 1);
        assert_ne!(new.ino(), now.ino());
        assert_eq!(fs::read_dir(o.path.join(".jmcl")).unwrap().count(), 1);
        o.check();
    }
    #[test]
    fn validation_precedes_blocked_parent_and_atomic_failure_cleans_temporary_file() {
        let o = Owned::new();
        fs::write(o.path.join(".jmcl"), b"blocked parent").unwrap();
        let before = snapshot(o.owner.path());
        assert_eq!(
            write(&o.root, &identity(b""), &target())
                .unwrap_err()
                .name(),
            "InvalidVersionId"
        );
        assert_eq!(snapshot(o.owner.path()), before);
        assert_eq!(
            write(&o.root, &identity(b"v"), &target())
                .unwrap_err()
                .name(),
            "FileIoError"
        );
        assert_eq!(snapshot(o.owner.path()), before);
        o.check();
        let o = Owned::new();
        fs::create_dir_all(o.path.join(".jmcl/install.json")).unwrap();
        let before = snapshot(o.owner.path());
        assert_eq!(
            write(&o.root, &identity(&[255]), &target())
                .unwrap_err()
                .name(),
            "FileIoError"
        );
        let mut after = snapshot(o.owner.path());
        let parent = PathBuf::from("instances/demo/.minecraft/.jmcl");
        let b = before.get(&parent).unwrap();
        let a = after.get_mut(&parent).unwrap();
        a.5 = b.5;
        a.6 = b.6;
        assert_eq!(after, before);
        assert_eq!(fs::read_dir(o.path.join(".jmcl")).unwrap().count(), 1);
        o.check();
    }
}

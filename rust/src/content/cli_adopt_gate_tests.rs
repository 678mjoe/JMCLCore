//! Explicit owned instance/CAS roots; synthetic opaque payloads only.
use super::*;
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
type Snapshot =
    std::collections::BTreeMap<std::path::PathBuf, (Vec<u8>, u64, u64, u64, u32, i64, i64)>;
fn snapshot(root: &Path) -> Snapshot {
    fn walk(root: &Path, path: &Path, out: &mut Snapshot) {
        let m = std::fs::symlink_metadata(path).unwrap();
        let bytes = if m.is_file() {
            std::fs::read(path).unwrap()
        } else {
            vec![]
        };
        out.insert(
            path.strip_prefix(root).unwrap().into(),
            (
                bytes,
                m.dev(),
                m.ino(),
                m.nlink(),
                m.mode(),
                m.mtime(),
                m.mtime_nsec(),
            ),
        );
        if m.is_dir() {
            for item in std::fs::read_dir(path).unwrap() {
                walk(root, &item.unwrap().path(), out);
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, root, &mut out);
    out
}

struct Fixture {
    owner: tempfile::TempDir,
    root: PathBuf,
    cas: PathBuf,
}
impl Fixture {
    fn new(manifest: &[u8], data: &[u8]) -> Self {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("instances");
        let cas = owner.path().join("cas");
        std::fs::create_dir_all(root.join("demo/.minecraft/mods")).unwrap();
        std::fs::write(root.join("demo/instance.json"), manifest).unwrap();
        std::fs::write(owner.path().join("peer"), data).unwrap();
        std::fs::set_permissions(
            owner.path().join("peer"),
            std::fs::Permissions::from_mode(0o444),
        )
        .unwrap();
        std::fs::hard_link(
            owner.path().join("peer"),
            root.join("demo/.minecraft/mods/a.jar"),
        )
        .unwrap();
        std::fs::write(owner.path().join("sentinel"), b"untouched").unwrap();
        std::fs::hard_link(
            owner.path().join("sentinel"),
            owner.path().join("sentinel-peer"),
        )
        .unwrap();
        Self { owner, root, cas }
    }
    fn request(
        &self,
        root: Option<&Path>,
        cas: Option<&Path>,
    ) -> std::result::Result<(u8, Vec<u8>, Vec<u8>), &'static str> {
        if root != Some(self.root.as_path())
            || cas != Some(self.cas.as_path())
            || self.root.parent() != Some(self.owner.path())
            || self.cas.parent() != Some(self.owner.path())
        {
            return Err("FIXTURE_GUARD");
        }
        let args = vec![
            "demo".into(),
            "--instances-directory".into(),
            self.root.to_str().unwrap().into(),
            "--store-directory".into(),
            self.cas.to_str().unwrap().into(),
        ];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = adopt(Kind::Mod, &args, &mut out, &mut err).unwrap();
        Ok((code, out, err))
    }
    fn call(&self) -> (u8, Vec<u8>, Vec<u8>) {
        self.request(Some(&self.root), Some(&self.cas)).unwrap()
    }
}
fn manifest() -> Vec<u8> {
    json!({"id":"demo","name":[255],"version_id":[254],"fabric_loader":[253]})
        .to_string()
        .into_bytes()
}
#[test]
fn guarded_opaque_adoption_preserves_inode_peers_registry_and_retained_lock() {
    let data = b"\xff\x00synthetic";
    let hash = format!("{:x}", Sha1::digest(data));
    let f = Fixture::new(&manifest(), data);
    let before = snapshot(f.owner.path());
    assert_eq!(
        f.call(),
        (
            0,
            format!(
                "adopted\tmods/a.jar\t{hash}\t{}\nadoptedCount\t1\n",
                data.len()
            )
            .into_bytes(),
            vec![]
        )
    );
    let after = snapshot(f.owner.path());
    for (path, state) in &after {
        if path.starts_with("cas") {
            eprintln!("retained CAS {path:?}: {state:?}");
        }
    }
    let mut expected = before[Path::new("peer")].clone();
    assert_eq!(expected.3, 2);
    expected.3 = 3;
    let object = format!("cas/objects/sha1/{}/{hash}", &hash[..2]);
    for path in [
        "peer",
        "instances/demo/.minecraft/mods/a.jar",
        object.as_str(),
    ] {
        assert_eq!(after[Path::new(path)], expected);
        assert_eq!(after[Path::new(path)].4 & 0o222, 0);
    }
    for path in ["sentinel", "sentinel-peer", "instances/demo/instance.json"] {
        assert_eq!(after[Path::new(path)], before[Path::new(path)]);
    }
    let lock = format!("cas/locks/sha1/{hash}.lock");
    assert!(after[Path::new(&lock)].0.is_empty());
    let registry = registry::Registry {
        schema_version: 1,
        entries: vec![registry::Entry {
            kind: Kind::Mod,
            file: "mods/a.jar".into(),
            sha1: hash,
            size: data.len() as u64,
            source: registry::Source {
                provider: registry::Provider::Manual,
                project_id: None,
                version_id: None,
                slug: None,
            },
            enabled: true,
            as_dependency: false,
        }],
    };
    let mut bytes = serde_json::to_vec(&registry).unwrap();
    bytes.push(b'\n');
    let registry_path = Path::new("instances/demo/.minecraft/.jmcl/content.json");
    assert_eq!(after[registry_path].0, bytes);
    assert_eq!(f.call(), (0, b"adoptedCount\t0\n".to_vec(), vec![]));
    let repeat = snapshot(f.owner.path());
    assert_eq!(
        repeat.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for path in [
        "peer",
        "instances/demo/.minecraft/mods/a.jar",
        object.as_str(),
        lock.as_str(),
        "sentinel",
        "sentinel-peer",
    ] {
        assert_eq!(repeat[Path::new(path)], after[Path::new(path)]);
    }
    assert_eq!(repeat[registry_path].0, bytes);
    assert_ne!(repeat[registry_path].2, after[registry_path].2); // Existing unconditional atomic save on repeat.
}
#[test]
fn adoption_guards_and_manifest_failures_do_not_open_cas() {
    for raw in [
        b"{".to_vec(),
        json!({"id":"other","name":"N","version_id":[255]})
            .to_string()
            .into_bytes(),
    ] {
        let f = Fixture::new(&raw, b"opaque");
        let before = snapshot(f.owner.path());
        for (root, cas) in [
            (None, Some(f.cas.as_path())),
            (Some(f.root.as_path()), None),
            (Some(Path::new("")), Some(f.cas.as_path())),
            (Some(f.root.as_path()), Some(Path::new(""))),
            (Some(Path::new("instances")), Some(Path::new(".jmclcore"))),
        ] {
            assert_eq!(f.request(root, cas), Err("FIXTURE_GUARD"));
        }
        let (code, out, err) = f.call();
        assert_eq!(code, 1);
        assert!(out.is_empty());
        let error = if raw == b"{" {
            "UnexpectedEndOfInput"
        } else {
            "InstanceIdMismatch"
        };
        assert_eq!(
            String::from_utf8(err).unwrap(),
            format!("failed to load instance: {error}\n")
        );
        assert_eq!(snapshot(f.owner.path()), before);
        assert!(!f.cas.exists());
    }
}
#[test]
fn empty_content_failure_retains_only_created_cas_layout() {
    let f = Fixture::new(&manifest(), b"");
    let before = snapshot(f.owner.path());
    assert_eq!(
        f.call(),
        (
            1,
            vec![],
            b"failed to adopt unmanaged content: InvalidContentSize\n".to_vec()
        )
    );
    let after = snapshot(f.owner.path());
    let added: Vec<_> = after
        .keys()
        .filter(|p| !before.contains_key(*p))
        .map(|p| p.to_str().unwrap())
        .collect();
    assert_eq!(
        added,
        vec![
            "cas",
            "cas/locks",
            "cas/locks/sha1",
            "cas/objects",
            "cas/objects/sha1"
        ]
    );
    for (path, old) in &before {
        if !path.as_os_str().is_empty() {
            assert_eq!(&after[path], old);
        }
    }
    assert!(!f.root.join("demo/.minecraft/.jmcl").exists());
}

#[test]
fn manifest_precedes_cas_root_failure_without_adoption_effects() {
    for valid in [false, true] {
        let bytes = if valid { manifest() } else { b"{".to_vec() };
        let f = Fixture::new(&bytes, b"opaque");
        std::fs::write(&f.cas, b"owned blocking file").unwrap();
        let before = snapshot(f.owner.path());
        let expected = if valid {
            "failed to open artifact store: NotDir\n"
        } else {
            "failed to load instance: UnexpectedEndOfInput\n"
        };
        assert_eq!(f.call(), (1, vec![], expected.as_bytes().to_vec()));
        assert_eq!(snapshot(f.owner.path()), before);
    }
}

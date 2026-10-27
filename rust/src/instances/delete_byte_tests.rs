//! Disposable-root delete regression fixtures; no ambient paths reach dispatch.
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

struct Fixture {
    owner: tempfile::TempDir,
    root: PathBuf,
}
#[derive(Debug, PartialEq, Eq)]
struct Snapshot(Vec<u8>, u64, u64, u64, u32);
fn snapshot(path: &Path) -> Snapshot {
    let m = fs::metadata(path).unwrap();
    Snapshot(
        fs::read(path).unwrap(),
        m.dev(),
        m.ino(),
        m.nlink(),
        m.mode(),
    )
}
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("instances");
        fs::create_dir_all(root.join("demo/.minecraft")).unwrap();
        fs::write(root.join("demo/instance.json"), bytes).unwrap();
        fs::write(root.join("demo/.minecraft/payload"), b"synthetic payload").unwrap();
        fs::set_permissions(
            root.join("demo/.minecraft/payload"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        fs::hard_link(
            root.join("demo/.minecraft/payload"),
            owner.path().join("payload-peer"),
        )
        .unwrap();
        fs::write(owner.path().join("sentinel"), b"outside target").unwrap();
        fs::hard_link(
            owner.path().join("sentinel"),
            owner.path().join("sentinel-peer"),
        )
        .unwrap();
        Self { owner, root }
    }
    fn input(&self) -> Value {
        json!({"directory":self.root,"id":"demo"})
    }
    fn request(&self, input: Value) -> Result<Value, RpcError> {
        if input.get("directory").and_then(Value::as_str) != self.root.to_str()
            || input.get("id").and_then(Value::as_str) != Some("demo")
            || self.root.parent() != Some(self.owner.path())
        {
            return Err(RpcError::new(
                "FIXTURE_GUARD",
                "explicit owned root and target required",
            ));
        }
        let raw = RawValue::from_string(input.to_string()).unwrap();
        handle("instance.delete", Some(&raw)).unwrap()
    }
    fn peers(&self) -> Vec<Snapshot> {
        ["payload-peer", "sentinel", "sentinel-peer"]
            .iter()
            .map(|p| snapshot(&self.owner.path().join(p)))
            .collect()
    }
    fn target(&self) -> Vec<Snapshot> {
        ["demo/instance.json", "demo/.minecraft/payload"]
            .iter()
            .map(|p| snapshot(&self.root.join(p)))
            .collect()
    }
}
fn manifest() -> Value {
    json!({"schema_version":1,"id":"demo","name":[255],"version_id":[254],"fabric_loader":[253],"source":"official"})
}
#[test]
fn guarded_opaque_delete_preserves_external_peers_and_repeat_error() {
    let f = Fixture::new(manifest().to_string().as_bytes());
    let mut peers = f.peers();
    assert_eq!(peers[0].3, 2);
    assert_eq!(
        f.request(f.input()).unwrap(),
        json!({"stage":"instance.delete","id":"demo","deleted":true})
    );
    assert!(!f.root.join("demo").exists());
    assert!(f.root.is_dir());
    peers[0].3 = 1;
    assert_eq!(f.peers(), peers);
    assert_eq!(f.request(f.input()).unwrap_err().code, "INSTANCE_NOT_FOUND");
    assert_eq!(f.peers(), peers);
}
#[test]
fn guarded_invalid_manifests_preserve_target_and_peers() {
    let mut cases = Vec::new();
    for (field, value, code) in [
        ("id", json!("other"), "INSTANCE_DELETE_FAILED"),
        ("id", json!("../bad"), "INSTANCE_NOT_FOUND"),
        ("name", json!([]), "INSTANCE_DELETE_FAILED"),
        ("version_id", json!([]), "INSTANCE_DELETE_FAILED"),
        ("fabric_loader", json!([]), "INSTANCE_DELETE_FAILED"),
        ("version_id", json!([256]), "INSTANCE_DELETE_FAILED"),
        ("forge_version", json!([252]), "INSTANCE_DELETE_FAILED"),
    ] {
        let mut m = manifest();
        m[field] = value;
        cases.push((m.to_string(), code));
    }
    cases.push(("{".into(), "INSTANCE_DELETE_FAILED"));
    for (bytes, code) in cases {
        let f = Fixture::new(bytes.as_bytes());
        let before = (f.target(), f.peers());
        assert_eq!(f.request(f.input()).unwrap_err().code, code, "{bytes}");
        assert_eq!((f.target(), f.peers()), before);
    }
}
#[test]
fn delete_driver_rejects_ambient_roots_and_targets_before_dispatch() {
    let f = Fixture::new(manifest().to_string().as_bytes());
    let before = (f.target(), f.peers());
    for input in [
        json!({}),
        json!({"directory":"","id":"demo"}),
        json!({"directory":".","id":"demo"}),
        json!({"directory":f.root}),
        json!({"directory":f.root,"id":"../sentinel"}),
    ] {
        assert_eq!(f.request(input).unwrap_err().code, "FIXTURE_GUARD");
        assert_eq!((f.target(), f.peers()), before);
    }
}

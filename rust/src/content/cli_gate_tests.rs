//! Owned synthetic gate fixtures. No install/adopt/provider paths.
use super::*;
use serde_json::json;
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

fn manifest() -> Value {
    json!({"id":"demo","name":[255],"version_id":[254],"fabric_loader":[253]})
}
fn registry(enabled: bool) -> Value {
    json!({"schema_version":1,"entries":[{"kind":"mod","file":"mods/a.jar","sha1":"1111111111111111111111111111111111111111","size":1,"source":{"provider":"manual","project_id":null,"version_id":null,"slug":null},"enabled":enabled,"as_dependency":false}]})
}
struct Fixture {
    owner: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("instances");
        let game = root.join("demo/.minecraft");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::create_dir(game.join(".jmcl")).unwrap();
        std::fs::write(root.join("demo/instance.json"), bytes).unwrap();
        std::fs::write(game.join(".jmcl/content.json"), registry(true).to_string()).unwrap();
        std::fs::write(owner.path().join("object-peer"), b"x").unwrap();
        std::fs::set_permissions(
            owner.path().join("object-peer"),
            std::fs::Permissions::from_mode(0o444),
        )
        .unwrap();
        std::fs::hard_link(owner.path().join("object-peer"), game.join("mods/a.jar")).unwrap();
        std::fs::write(owner.path().join("sentinel"), b"sibling").unwrap();
        std::fs::hard_link(
            owner.path().join("sentinel"),
            owner.path().join("sentinel-peer"),
        )
        .unwrap();
        Self { owner, root }
    }
    fn request(&self, op: &str, root: &Path, id: &str, file: &str) -> (u8, Vec<u8>, Vec<u8>) {
        assert!(matches!(op, "enable" | "disable" | "remove"));
        assert_eq!(root, self.root);
        assert_eq!(root.parent(), Some(self.owner.path()));
        assert_eq!(id, "demo");
        assert_eq!(file, "mods/a.jar");
        let args = [
            LocalArg::Text(id),
            LocalArg::Text("--instances-directory"),
            LocalArg::Text(root.to_str().unwrap()),
            LocalArg::Text("--file"),
            LocalArg::Text(file),
        ];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = local_inner(Kind::Mod, op, &args, &mut out, &mut err).unwrap();
        (code, out, err)
    }
    fn call(&self, op: &str) -> (u8, Vec<u8>, Vec<u8>) {
        self.request(op, &self.root, "demo", "mods/a.jar")
    }
    fn assert_registry(&self, expected: Value) {
        let path = self.root.join("demo/.minecraft/.jmcl/content.json");
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
            expected
        );
    }
}
#[test]
fn guarded_opaque_toggle_remove_preserves_readonly_peers() {
    let f = Fixture::new(manifest().to_string().as_bytes());
    let before = snapshot(f.owner.path());
    let game = f.root.join("demo/.minecraft");
    assert_eq!(
        f.call("disable"),
        (0, b"disabled\tmods/a.jar\n".to_vec(), vec![])
    );
    f.assert_registry(registry(false));
    assert!(!game.join("mods/a.jar").exists());
    assert!(game.join("mods/a.jar.disabled").exists());
    let disabled = snapshot(f.owner.path());
    assert_eq!(
        f.call("disable"),
        (0, b"unchanged\tmods/a.jar\tdisabled\n".to_vec(), vec![])
    );
    assert_eq!(snapshot(f.owner.path()), disabled);
    assert_eq!(
        f.call("enable"),
        (0, b"enabled\tmods/a.jar\n".to_vec(), vec![])
    );
    f.assert_registry(registry(true));
    assert!(game.join("mods/a.jar").exists());
    assert!(!game.join("mods/a.jar.disabled").exists());
    let enabled = snapshot(f.owner.path());
    assert_eq!(
        f.call("enable"),
        (0, b"unchanged\tmods/a.jar\tenabled\n".to_vec(), vec![])
    );
    assert_eq!(snapshot(f.owner.path()), enabled);
    for key in [
        "object-peer",
        "sentinel",
        "sentinel-peer",
        "instances/demo/instance.json",
    ] {
        assert_eq!(enabled[Path::new(key)], before[Path::new(key)]);
    }
    assert_eq!(
        enabled[Path::new("instances/demo/.minecraft/mods/a.jar")],
        before[Path::new("object-peer")]
    );
    assert_eq!(
        f.call("remove"),
        (
            0,
            b"removed\tmods/a.jar\t1111111111111111111111111111111111111111\n".to_vec(),
            vec![]
        )
    );
    f.assert_registry(json!({"schema_version":1,"entries":[]}));
    assert!(!game.join("mods/a.jar").exists());
    let after = snapshot(f.owner.path());
    let mut peer = before[Path::new("object-peer")].clone();
    assert_eq!(peer.3, 2);
    peer.3 = 1;
    assert_eq!(after[Path::new("object-peer")], peer);
    for key in ["sentinel", "sentinel-peer", "instances/demo/instance.json"] {
        assert_eq!(after[Path::new(key)], before[Path::new(key)]);
    }
    for op in ["remove", "enable", "disable"] {
        assert_eq!(
            f.call(op),
            (
                1,
                vec![],
                b"no registered mod matches the given selector\n".to_vec()
            )
        );
        assert_eq!(snapshot(f.owner.path()), after);
    }
}
#[test]
fn guarded_malformed_and_mismatched_manifests_prevent_content_changes() {
    let mut mismatch = manifest();
    mismatch["id"] = json!("other");
    let mut empty = manifest();
    empty["version_id"] = json!([]);
    for (raw, error) in [
        ("{".to_owned(), "SyntaxError"),
        (mismatch.to_string(), "InstanceIdMismatch"),
        (empty.to_string(), "InvalidVersionId"),
    ] {
        let f = Fixture::new(raw.as_bytes());
        let before = snapshot(f.owner.path());
        for op in ["enable", "disable", "remove"] {
            let (code, out, err) = f.call(op);
            assert_eq!(code, 1);
            assert!(out.is_empty());
            assert_eq!(
                String::from_utf8(err).unwrap(),
                format!("failed to load instance: {error}\n")
            );
            assert_eq!(snapshot(f.owner.path()), before);
        }
    }
}

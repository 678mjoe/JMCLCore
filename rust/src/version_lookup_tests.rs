//! Owned metadata caches and strictly queued fake metadata responses only.
use super::*;
use serde_json::json;
use std::{cell::RefCell, collections::VecDeque, os::unix::fs::MetadataExt};
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

const URL: &str = "https://example.invalid/metadata";
struct Fake {
    replies: RefCell<VecDeque<Result<Vec<u8>, Error>>>,
    calls: RefCell<usize>,
}
impl Transport for Fake {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, Error> {
        assert_eq!(url, URL);
        assert_eq!(max, 4 * 1024 * 1024);
        *self.calls.borrow_mut() += 1;
        self.replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected metadata request")
    }
}
fn fixture(replies: Vec<Result<Vec<u8>, Error>>) -> (tempfile::TempDir, Service<Fake>) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("cache");
    fs::create_dir(&root).unwrap();
    fs::write(owner.path().join("sentinel"), b"unmodified").unwrap();
    fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
    let mut svc = Service::new(
        Fake {
            replies: RefCell::new(replies.into()),
            calls: RefCell::new(0),
        },
        root,
    );
    svc.now = 1000;
    (owner, svc)
}
fn seed(svc: &Service<Fake>, url: &str, bytes: &[u8]) {
    let e = Envelope {
        v: 1,
        url: url.into(),
        fetched_at: 1000,
        payload: Payload::Text(String::from_utf8(bytes.to_vec()).unwrap()),
    };
    fs::write(svc.cache_path(url), serde_json::to_vec(&e).unwrap()).unwrap();
}
fn body(id: &str) -> Vec<u8> {
    json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/not-fetched"}}}).to_string().into_bytes()
}
fn manifest(url: Value, hash: Value) -> Value {
    json!({"latest":{"release":"demo","snapshot":[255]},"versions":[
     {"id":[255],"type":"release","url":[254],"sha1":[253],"time":"t","releaseTime":"r"},
     {"id":"demo","type":"release","url":url,"sha1":hash,"time":"t","releaseTime":"r"},
     {"id":"demo","type":"release","url":"https://example.invalid/wrong-duplicate","time":"t","releaseTime":"r"},
     {"id":[252],"type":"release","url":[251],"time":"t","releaseTime":"r"}
    ]})
}
fn seed_manifest(svc: &Service<Fake>, m: &Value) {
    seed(
        svc,
        Source::Official.manifest_url(),
        m.to_string().as_bytes(),
    );
}
fn assert_cache_change_only(owner: &Path, svc: &Service<Fake>, before: Snapshot, expected: &[u8]) {
    let path = svc.cache_path(URL);
    let env: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(env.payload.bytes(), expected);
    assert_eq!(env.url, URL);
    assert_eq!(env.v, 1);
    assert_eq!(env.fetched_at, 1000);
    let mut after = snapshot(owner);
    let mut before = before;
    let rel = path.strip_prefix(owner).unwrap();
    after.remove(rel);
    before.remove(rel);
    let key = Path::new("cache");
    let original = &before[key];
    let current = after.get_mut(key).unwrap();
    current.5 = original.5;
    current.6 = original.6;
    assert_eq!(after, before);
}
#[test]
fn supported_cached_get_ignores_unrelated_opaque_ids_and_uses_first_match() {
    let (owner, svc) = fixture(vec![]);
    let payload = body("demo");
    let hash = format!("{:x}", Sha1::digest(&payload));
    seed_manifest(&svc, &manifest(json!(URL), json!(hash)));
    seed(&svc, URL, &payload);
    let before = snapshot(owner.path());
    assert_eq!(svc.get("demo", Source::Official).unwrap()["id"], "demo");
    assert_eq!(
        handle_with(&svc, "version.get", Some(&json!({"id":"demo"})))
            .unwrap()
            .result["id"],
        "demo"
    );
    assert_eq!(*svc.transport.calls.borrow(), 0);
    assert_eq!(snapshot(owner.path()), before);
    assert!(svc.manifest(Source::Official).is_err());
}
#[test]
fn parse_absence_and_selected_url_boundary_precede_metadata_transport() {
    for case in 0..3 {
        let (owner, svc) = fixture(vec![]);
        let mut m = manifest(json!([255]), Value::Null);
        let (id, expected) = match case {
            0 => ("absent", Error::VersionNotFound),
            1 => ("demo", Error::Json("UnexpectedToken")),
            _ => {
                m["versions"][3]["type"] = json!("invalid");
                ("demo", Error::Json("InvalidEnumTag"))
            }
        };
        seed_manifest(&svc, &m);
        let before = snapshot(owner.path());
        assert_eq!(svc.get(id, Source::Official), Err(expected));
        assert_eq!(*svc.transport.calls.borrow(), 0);
        assert_eq!(snapshot(owner.path()), before);
    }
}
#[test]
fn opaque_sha1_is_never_absent_and_hash_failure_precedes_metadata_parse() {
    let (owner, svc) = fixture(vec![Ok(b"{".to_vec())]);
    seed_manifest(&svc, &manifest(json!(URL), json!([255])));
    let before = snapshot(owner.path());
    assert_eq!(
        svc.get("demo", Source::Official),
        Err(Error::MetadataHashMismatch)
    );
    assert_eq!(*svc.transport.calls.borrow(), 1);
    assert!(svc.transport.replies.borrow().is_empty());
    assert_cache_change_only(owner.path(), &svc, before, b"{");
}
#[test]
fn cached_hash_retry_and_final_identity_check_keep_existing_order() {
    for opaque_hash in [false, true] {
        let payload = body("demo");
        let (owner, svc) = fixture(vec![Ok(payload.clone())]);
        let hash = if opaque_hash {
            json!([255])
        } else {
            json!(format!("{:x}", Sha1::digest(&payload)))
        };
        seed_manifest(&svc, &manifest(json!(URL), hash));
        seed(&svc, URL, b"{");
        let before = snapshot(owner.path());
        let result = svc.get("demo", Source::Official);
        if opaque_hash {
            assert_eq!(result, Err(Error::MetadataHashMismatch));
        } else {
            assert_eq!(result.unwrap()["id"], "demo");
        }
        assert_eq!(*svc.transport.calls.borrow(), 1);
        assert!(svc.transport.replies.borrow().is_empty());
        assert_cache_change_only(owner.path(), &svc, before, &payload);
    }
    let payload = body("different");
    let (owner, svc) = fixture(vec![]);
    seed_manifest(
        &svc,
        &manifest(json!(URL), json!(format!("{:x}", Sha1::digest(&payload)))),
    );
    seed(&svc, URL, &payload);
    let before = snapshot(owner.path());
    assert_eq!(
        svc.get("demo", Source::Official),
        Err(Error::MetadataIdMismatch)
    );
    assert_eq!(*svc.transport.calls.borrow(), 0);
    assert_eq!(snapshot(owner.path()), before);
}

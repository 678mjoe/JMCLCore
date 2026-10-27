//! Owned metadata cache only; transport calls must be explicitly queued.
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

struct Guarded {
    source: Source,
    responses: RefCell<VecDeque<Result<Vec<u8>, Error>>>,
    calls: RefCell<usize>,
}
impl Transport for Guarded {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, Error> {
        assert_eq!(url, self.source.manifest_url());
        assert_eq!(max, 2 * 1024 * 1024);
        *self.calls.borrow_mut() += 1;
        self.responses
            .borrow_mut()
            .pop_front()
            .expect("unexpected transport request")
    }
}
fn fixture(
    source: Source,
    responses: Vec<Result<Vec<u8>, Error>>,
) -> (tempfile::TempDir, Service<Guarded>) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("metadata-cache");
    fs::create_dir(&root).unwrap();
    fs::write(owner.path().join("sentinel"), b"outside cache").unwrap();
    fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
    let mut svc = Service::new(
        Guarded {
            source,
            responses: RefCell::new(responses.into()),
            calls: RefCell::new(0),
        },
        root,
    );
    svc.now = 1000;
    (owner, svc)
}
fn seed(svc: &Service<Guarded>, source: Source, payload: &[u8]) {
    let env = Envelope {
        v: 1,
        url: source.manifest_url().into(),
        fetched_at: 1000,
        payload: Payload::Text(String::from_utf8(payload.to_vec()).unwrap()),
    };
    fs::write(
        svc.cache_path(source.manifest_url()),
        serde_json::to_vec(&env).unwrap(),
    )
    .unwrap();
}
fn manifest() -> Vec<u8> {
    let versions: Vec<_> = (0..22).map(|i| json!({"id":[255,i],"type":if i%2==0 {"release"} else {"snapshot"},"url":"synthetic-url","time":"t","releaseTime":[254]})).collect();
    serde_json::to_vec(&json!({"latest":{"release":[255],"snapshot":"s"},"versions":versions}))
        .unwrap()
}
#[test]
fn cached_opaque_rpc_and_cli_listing_preserve_cache_and_siblings() {
    for source in [Source::Official, Source::Bmclapi] {
        let (owner, svc) = fixture(source, vec![]);
        seed(&svc, source, &manifest());
        let before = snapshot(owner.path());
        let params = json!({"source":source});
        let response = handle_with(&svc, "version.list", Some(&params)).unwrap();
        assert_eq!(response.source, source);
        assert_eq!(response.result["versions"].as_array().unwrap().len(), 20);
        assert_eq!(response.result["versions"][0]["id"], json!([255, 0]));
        let params = json!({"source":source,"type":"snapshot","limit":1});
        let response = handle_with(&svc, "version.list", Some(&params)).unwrap();
        assert_eq!(response.result["versions"].as_array().unwrap().len(), 1);
        assert_eq!(response.result["versions"][0]["id"], json!([255, 1]));
        for encoded in [false, true] {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            assert_eq!(
                cli_execute(&svc, "version.list", &params, encoded, &mut out, &mut err).unwrap(),
                0
            );
            assert!(err.is_empty());
            if encoded {
                assert_eq!(
                    serde_json::from_slice::<Value>(&out).unwrap(),
                    json!({"protocol":1,"source":source,"result":response.result})
                );
            } else {
                let mut expected = if source == Source::Bmclapi {
                    b"# source\tBMCLAPI\n".to_vec()
                } else {
                    vec![]
                };
                expected.extend_from_slice(b"\xff\x01\tsnapshot\t\xfe\n");
                assert_eq!(out, expected);
            }
        }
        assert!(svc.manifest(source).is_err()); // Operational parser stays String-based.
        assert_eq!(*svc.transport.calls.borrow(), 0);
        assert_eq!(snapshot(owner.path()), before);
    }
}
#[test]
fn invalid_cached_payload_is_reported_without_eviction_or_transport() {
    let (owner, svc) = fixture(Source::Official, vec![]);
    seed(&svc, Source::Official, b"{");
    let before = snapshot(owner.path());
    assert_eq!(
        handle_with(&svc, "version.list", None).unwrap_err().code,
        "INVALID_MANIFEST"
    );
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        cli_execute(&svc, "version.list", &json!({}), false, &mut out, &mut err).unwrap(),
        1
    );
    assert!(out.is_empty());
    assert_eq!(
        err,
        b"failed to fetch version manifest from Mojang: SyntaxError\n"
    );
    assert_eq!(*svc.transport.calls.borrow(), 0);
    assert_eq!(snapshot(owner.path()), before);
}
#[test]
fn missing_and_invalid_envelope_use_only_expected_rejecting_transport() {
    for invalid in [false, true] {
        let (owner, svc) = fixture(Source::Official, vec![Err(Error::Network)]);
        let path = svc.cache_path(Source::Official.manifest_url());
        if invalid {
            fs::write(&path, b"invalid envelope").unwrap();
        }
        let before = snapshot(owner.path());
        assert_eq!(
            handle_with(&svc, "version.list", None).unwrap_err().code,
            "NETWORK_ERROR"
        );
        assert_eq!(*svc.transport.calls.borrow(), 1);
        assert!(svc.transport.responses.borrow().is_empty());
        assert!(!path.exists());
        let after = snapshot(owner.path());
        // Only malformed-envelope eviction is allowed. Directory mtime can change.
        let mut expected = before;
        if invalid {
            expected.remove(path.strip_prefix(owner.path()).unwrap());
            let key = std::path::PathBuf::from("metadata-cache");
            let now = &after[&key];
            let was = expected.get_mut(&key).unwrap();
            was.5 = now.5;
            was.6 = now.6;
        }
        assert_eq!(after, expected);
    }
}
#[test]
fn synthetic_manifest_response_populates_only_expected_cache_envelope() {
    let bytes = manifest();
    let (owner, svc) = fixture(Source::Official, vec![Ok(bytes.clone())]);
    let before = snapshot(owner.path());
    let result = handle_with(&svc, "version.list", None).unwrap();
    assert_eq!(result.result["versions"][0]["id"], json!([255, 0]));
    assert_eq!(*svc.transport.calls.borrow(), 1);
    assert!(svc.transport.responses.borrow().is_empty());
    let path = svc.cache_path(Source::Official.manifest_url());
    let env: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        (env.v, env.url, env.fetched_at, env.payload.bytes()),
        (1, Source::Official.manifest_url().into(), 1000, bytes)
    );
    let mut after = snapshot(owner.path());
    after.remove(path.strip_prefix(owner.path()).unwrap());
    let key = std::path::PathBuf::from("metadata-cache");
    let was = &before[&key];
    let now = after.get_mut(&key).unwrap();
    now.5 = was.5;
    now.6 = was.6;
    assert_eq!(after, before);
}

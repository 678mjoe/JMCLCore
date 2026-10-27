//! Read endpoint fixtures: explicit owned cache and strictly queued responses.
use super::*;
use serde_json::json;
use std::{
    cell::RefCell,
    collections::VecDeque,
    os::unix::{ffi::OsStringExt, fs::MetadataExt},
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
            .expect("unexpected read transport request")
    }
}
fn fixture(replies: Vec<Result<Vec<u8>, Error>>) -> (tempfile::TempDir, Service<Fake>) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("cache");
    fs::create_dir(&root).unwrap();
    fs::write(owner.path().join("sentinel"), b"unchanged").unwrap();
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
    let env = Envelope {
        v: 1,
        url: url.into(),
        fetched_at: 1000,
        payload: Payload::Text(String::from_utf8(bytes.to_vec()).unwrap()),
    };
    fs::write(svc.cache_path(url), serde_json::to_vec(&env).unwrap()).unwrap();
}
fn body(id: Value) -> Vec<u8> {
    json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/never-fetched"}}}).to_string().into_bytes()
}
fn manifest(id: Value, url: Value, hash: Value) -> Vec<u8> {
    json!({"latest":{"release":[254],"snapshot":"s"},"versions":[{"id":[254],"type":"release","url":[255],"time":"t","releaseTime":"r"},{"id":id,"type":"release","url":url,"sha1":hash,"time":"t","releaseTime":"r"},{"id":id,"type":"release","url":"https://example.invalid/duplicate-must-not-win","time":"t","releaseTime":"r"}]}).to_string().into_bytes()
}
fn setup(svc: &Service<Fake>, source: Source, id: Value, bytes: &[u8]) {
    seed(
        svc,
        source.manifest_url(),
        &manifest(id, json!(URL), json!(format!("{:x}", Sha1::digest(bytes)))),
    );
    seed(svc, URL, bytes);
}
#[test]
fn request_schema_precedes_empty_and_keeps_operational_routes_string_based() {
    let p = prepare(Some(
        &json!({"id":[255,0,"65",65.00000000000000001],"source":"0_1"}),
    ))
    .unwrap();
    assert_eq!(p.id, vec![255, 0, 65, 65]);
    assert_eq!(p.source, Source::Bmclapi);
    assert_eq!(
        prepare(None).err().unwrap(),
        invalid("version.get requires an id")
    );
    for value in [
        Value::Null,
        json!({}),
        json!({"id":null}),
        json!({"id":[256]}),
        json!({"id":[],"source":"bad"}),
    ] {
        assert_eq!(
            prepare(Some(&value)).err().unwrap(),
            param_error("version.get")
        );
    }
    assert_eq!(
        prepare(Some(&json!({"id":[]}))).err().unwrap(),
        invalid("version id must not be empty")
    );
    for method in ["version.resolve", "install.plan"] {
        assert!(canonical_params_view(method, Some(&json!({"id":[255]}))).is_err());
    }
}
#[test]
fn read_metadata_varies_only_top_level_id_under_complete_schema() {
    let utf8 = body(json!([100, 101, 109, 111]));
    let read = metadata::ReadMetadata::parse(&utf8).unwrap();
    assert_eq!(read.value(), &metadata::metadata(&utf8).unwrap());
    assert_eq!(read.id(), b"demo");
    let opaque = body(json!([255]));
    assert_eq!(metadata::ReadMetadata::parse(&opaque).unwrap().id(), &[255]);
    assert!(metadata::metadata(&opaque).is_err());
    for (path, value, error) in [
        ("size", json!(-1), "Overflow"),
        ("url", json!([255]), "UnexpectedToken"),
    ] {
        let mut v: Value = serde_json::from_slice(&opaque).unwrap();
        v["downloads"]["client"][path] = value;
        assert_eq!(
            metadata::ReadMetadata::parse(v.to_string().as_bytes()).err(),
            Some(Error::Json(error))
        );
    }
    let mut v: Value = serde_json::from_slice(&opaque).unwrap();
    v["downloads"].as_object_mut().unwrap().remove("client");
    assert_eq!(
        metadata::ReadMetadata::parse(v.to_string().as_bytes()).err(),
        Some(Error::Json("MissingField"))
    );
    let text = String::from_utf8(opaque)
        .unwrap()
        .replace("\"id\":[255]", "\"id\":[255],\"id\":[254]");
    assert_eq!(
        metadata::ReadMetadata::parse(text.as_bytes()).err(),
        Some(Error::Json("DuplicateField"))
    );
}
#[test]
fn cached_opaque_rpc_first_match_preserves_cache_and_string_api() {
    for source in [Source::Official, Source::Bmclapi] {
        let (owner, svc) = fixture(vec![]);
        let bytes = body(json!([255]));
        setup(&svc, source, json!([255]), &bytes);
        let before = snapshot(owner.path());
        let response = handle_with(
            &svc,
            "version.get",
            Some(&json!({"id":[255],"source":source})),
        )
        .unwrap();
        assert_eq!(response.source, source);
        assert_eq!(response.result["id"], json!([255]));
        assert_eq!(snapshot(owner.path()), before);
        assert_eq!(*svc.transport.calls.borrow(), 0);
    }
    let (owner, svc) = fixture(vec![]);
    let bytes = body(json!("demo"));
    setup(&svc, Source::Official, json!("demo"), &bytes);
    let before = snapshot(owner.path());
    assert_eq!(
        svc.get("demo", Source::Official).unwrap(),
        handle_with(&svc, "version.get", Some(&json!({"id":"demo"})))
            .unwrap()
            .result
    );
    assert_eq!(snapshot(owner.path()), before);
}
#[test]
fn opaque_read_errors_preserve_hash_parse_identity_and_no_fallback() {
    for case in 0..4 {
        let (owner, svc) = fixture(vec![]);
        let source = Source::Bmclapi;
        let bytes = if case == 1 {
            b"{".to_vec()
        } else {
            body(json!([254]))
        };
        let url = if case == 3 { json!([255]) } else { json!(URL) };
        seed(
            &svc,
            source.manifest_url(),
            &manifest(
                json!([255]),
                url,
                json!(format!("{:x}", Sha1::digest(&bytes))),
            ),
        );
        seed(&svc, URL, &bytes);
        let before = snapshot(owner.path());
        let id = if case == 0 {
            json!([253])
        } else {
            json!([255])
        };
        let error =
            handle_with(&svc, "version.get", Some(&json!({"id":id,"source":source}))).unwrap_err();
        assert_eq!(
            error.code,
            if case == 0 {
                "VERSION_NOT_FOUND"
            } else {
                "INVALID_METADATA"
            }
        );
        assert_eq!(*svc.transport.calls.borrow(), 0);
        assert_eq!(snapshot(owner.path()), before);
    }
    let (owner, svc) = fixture(vec![Err(Error::Network)]);
    seed(
        &svc,
        Source::Bmclapi.manifest_url(),
        &manifest(json!([255]), json!(URL), Value::Null),
    );
    let before = snapshot(owner.path());
    assert_eq!(
        handle_with(
            &svc,
            "version.get",
            Some(&json!({"id":[255],"source":"bmclapi"}))
        )
        .unwrap_err()
        .code,
        "NETWORK_ERROR"
    );
    assert_eq!(*svc.transport.calls.borrow(), 1);
    assert!(svc.transport.replies.borrow().is_empty());
    assert_eq!(snapshot(owner.path()), before);
}
#[test]
fn byte_read_cached_hash_retry_changes_only_expected_entry() {
    let bytes = body(json!([255]));
    let (owner, svc) = fixture(vec![Ok(bytes.clone())]);
    seed(
        &svc,
        Source::Official.manifest_url(),
        &manifest(
            json!([255]),
            json!(URL),
            json!(format!("{:x}", Sha1::digest(&bytes))),
        ),
    );
    seed(&svc, URL, b"{");
    let before = snapshot(owner.path());
    assert_eq!(
        handle_with(&svc, "version.get", Some(&json!({"id":[255]})))
            .unwrap()
            .result["id"],
        json!([255])
    );
    assert_eq!(*svc.transport.calls.borrow(), 1);
    assert!(svc.transport.replies.borrow().is_empty());
    let mut after = snapshot(owner.path());
    let mut before = before;
    let path = svc.cache_path(URL);
    let env: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(env.payload.bytes(), bytes);
    let rel = path.strip_prefix(owner.path()).unwrap();
    after.remove(rel);
    before.remove(rel);
    let old = &before[Path::new("cache")];
    let new = after.get_mut(Path::new("cache")).unwrap();
    new.5 = old.5;
    new.6 = old.6;
    assert_eq!(after, before);
    let (owner, svc) = fixture(vec![Ok(b"{".to_vec())]);
    seed(
        &svc,
        Source::Official.manifest_url(),
        &manifest(json!([255]), json!(URL), json!([254])),
    );
    assert_eq!(
        handle_with(&svc, "version.get", Some(&json!({"id":[255]})))
            .unwrap_err()
            .code,
        "INTEGRITY_ERROR"
    );
    assert_eq!(*svc.transport.calls.borrow(), 1);
    assert!(owner.path().join("sentinel").is_file());
}
#[test]
fn native_show_parser_and_injected_formatter_preserve_raw_id_only() {
    use std::ffi::OsString;
    let raw = OsString::from_vec(vec![255]);
    let (owner, svc) = fixture(vec![]);
    let bytes = body(json!([255]));
    setup(&svc, Source::Bmclapi, json!([255]), &bytes);
    let before = snapshot(owner.path());
    for encoded in [false, true] {
        let mut args = vec![
            "version".into(),
            "show".into(),
            raw.clone(),
            "--source".into(),
            "bmclapi".into(),
        ];
        if encoded {
            args.push("--json".into());
        }
        let (request, json) = native_request(&args).unwrap().unwrap();
        let (mut out, mut err) = (vec![], vec![]);
        assert_eq!(
            execute(&svc, &request, json, &mut out, &mut err).unwrap(),
            0
        );
        assert!(err.is_empty());
        if encoded {
            let result: Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(result["result"]["id"], json!([255]));
            assert_eq!(result["source"], "bmclapi");
        } else {
            assert_eq!(out,b"# source\tBMCLAPI\nid\t\xff\ntype\trelease\nmainClass\tMain\nlibraries\t0\nclient\thttps://example.invalid/never-fetched\narguments\tlegacy\n");
        }
    }
    let (mut out, mut err) = (vec![], vec![]);
    assert_eq!(
        execute(
            &svc,
            &Request {
                id: vec![253],
                source: Source::Bmclapi
            },
            false,
            &mut out,
            &mut err
        )
        .unwrap(),
        1
    );
    assert!(out.is_empty());
    assert_eq!(err, b"Minecraft version not found: \xfd\n");
    assert_eq!(snapshot(owner.path()), before);
    assert_eq!(*svc.transport.calls.borrow(), 0);
    for args in [
        vec!["install".into(), "plan".into(), raw.clone()],
        vec!["version".into(), "resolve".into(), raw.clone()],
        vec!["version".into(), "list".into(), raw.clone()],
        vec!["version".into(), "show".into(), "demo".into()],
    ] {
        assert!(native_request(&args).is_none());
    }
    assert!(
        native_request(&[
            "version".into(),
            "show".into(),
            raw,
            "--source".into(),
            "bad".into()
        ])
        .unwrap()
        .is_err()
    );
}

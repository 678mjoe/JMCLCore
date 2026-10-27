//! Read endpoint fixtures: explicit owned cache and strictly queued responses.
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

const META: &str = "https://piston-meta.mojang.com/test/metadata";
const ASSET: &str = "https://resources.download.minecraft.net/test/index";
type Reply = (String, usize, Result<Vec<u8>, Error>);
struct Fake {
    replies: RefCell<VecDeque<Reply>>,
    calls: RefCell<Vec<(String, usize)>>,
}
impl Transport for Fake {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, Error> {
        self.calls.borrow_mut().push((url.into(), max));
        let (expected, limit, result) = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected transport request");
        assert_eq!((url, max), (expected.as_str(), limit));
        result
    }
}
fn fixture(replies: Vec<Reply>) -> (tempfile::TempDir, Service<Fake>) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("cache");
    assert!(root.starts_with(owner.path()) && !root.as_os_str().is_empty());
    fs::create_dir(&root).unwrap();
    fs::write(owner.path().join("canary"), b"unchanged").unwrap();
    fs::hard_link(owner.path().join("canary"), owner.path().join("peer")).unwrap();
    let mut svc = Service::new(
        Fake {
            replies: RefCell::new(replies.into()),
            calls: RefCell::new(vec![]),
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
fn rewritten(source: Source, url: &str) -> String {
    source.rewrite(url).unwrap_or_else(|| url.into())
}
fn body(id: Value, asset: Option<&[u8]>) -> Vec<u8> {
    let mut v = json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"a".repeat(40),"size":1,"url":"https://piston-data.mojang.com/never-fetched"}}});
    if let Some(bytes) = asset {
        v["assetIndex"] = json!({"id":"index","url":ASSET,"sha1":format!("{:x}",Sha1::digest(bytes)),"size":bytes.len(),"totalSize":0});
    }
    serde_json::to_vec(&v).unwrap()
}
fn setup(svc: &Service<Fake>, source: Source, id: Value, bytes: &[u8]) {
    let manifest = json!({"latest":{"release":[254],"snapshot":"s"},"versions":[{"id":[254],"type":"release","url":[255],"time":"t","releaseTime":"r"},{"id":id,"type":"release","url":META,"sha1":format!("{:x}",Sha1::digest(bytes)),"time":"t","releaseTime":"r"},{"id":id,"type":"release","url":"https://example.invalid/must-not-win","time":"t","releaseTime":"r"}]});
    seed(
        svc,
        source.manifest_url(),
        &serde_json::to_vec(&manifest).unwrap(),
    );
    seed(svc, &rewritten(source, META), bytes);
}
fn raw(s: &str) -> Box<RawValue> {
    serde_json::from_str(s).unwrap()
}
fn invoke(svc: &Service<Fake>, p: &Value) -> Result<Response, RpcError> {
    handle_with(svc, "install.plan", Some(p))
}
fn asset_reply(source: Source, result: Result<Vec<u8>, Error>) -> Reply {
    (rewritten(source, ASSET), 16 * 1024 * 1024, result)
}
#[test]
fn raw_schema_and_exact_loader_exclusion_preserve_dynamic_contract() {
    let p = raw(
        r#"{"id":[255,"65",65.00000000000000001],"source":"0_1","os":1,"arch":"1","ignored":1e9999,"fabric_loader":null}"#,
    );
    assert!(selected(Some(&p)));
    let r = prepare(Some(&p)).unwrap();
    assert_eq!(r.id, vec![255, 65, 65]);
    assert_eq!(r.source, Source::Bmclapi);
    assert_eq!(r.os.as_deref(), Some("linux"));
    assert_eq!(r.arch.as_deref(), Some("x86_64"));
    for s in [
        r#"{"id":[],"os":"bad"}"#,
        r#"{"id":[],"source":null}"#,
        r#"{"id":[256]}"#,
        r#"{"id":[],"arch":1.0}"#,
        r#"{"id":"x","id":"y"}"#,
    ] {
        assert_eq!(
            prepare(Some(&raw(s))).err().unwrap(),
            param_error("install.plan")
        );
    }
    assert_eq!(
        prepare(Some(&raw(r#"{"id":[],"ignored":1e9999}"#)))
            .err()
            .unwrap(),
        invalid("version id must not be empty")
    );
    assert!(!selected(Some(&raw(r#"{"id":"x","ignored":"\ud800"}"#))));
    for value in [
        json!({"id":[255],"fabric_loader":""}),
        json!({"id":"x","fabric_loader":"0.1"}),
        json!({"id":"x","neoforge_version":[]}),
        json!({"id":"x","forge_version":false}),
        json!({"id":"x","fabric_loader":"a","forge_version":"b"}),
    ] {
        let text = raw(&value.to_string());
        assert!(!selected(Some(&text)));
        assert!(!selected_value(Some(&value)));
    }
    assert!(selected(Some(&raw(
        r#"{"id":"x","fabric_loader":null,"neoforge_version":null,"forge_version":null}"#
    ))));
    assert!(!selected(Some(&raw(
        r#"{"id":"x","fabric_loader":null,"fabric_loader":null}"#
    ))));
    // Protocol's existing duplicate/Unicode gate remains unchanged and precedes dispatch.
}
#[test]
fn cached_vanilla_rpc_cli_preserve_outputs_and_owned_cache() {
    for id in [json!("demo"), json!("版本"), json!([255])] {
        for source in [Source::Official, Source::Bmclapi] {
            let (owner, svc) = fixture(vec![]);
            let bytes = body(id.clone(), None);
            setup(&svc, source, id.clone(), &bytes);
            let before = snapshot(owner.path());
            let p = json!({"id":id,"source":source,"os":"linux","arch":"x86_64"});
            let response = invoke(&svc, &p).unwrap();
            assert_eq!(response.source, source);
            let parsed = metadata::ReadMetadata::parse(&bytes).unwrap();
            let expected = serde_json::to_value(
                plan::build_read(
                    &parsed,
                    None,
                    source,
                    Target {
                        os: "linux".into(),
                        arch: "x86_64".into(),
                        os_version: None,
                    },
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(response.result, expected);
            let mut out = vec![];
            let mut err = vec![];
            assert_eq!(
                cli_execute(&svc, "install.plan", &p, true, &mut out, &mut err).unwrap(),
                0
            );
            assert!(err.is_empty());
            assert_eq!(
                serde_json::from_slice::<Value>(&out).unwrap(),
                json!({"protocol":1,"source":source,"result":expected})
            );
            out.clear();
            assert_eq!(
                cli_execute(&svc, "install.plan", &p, false, &mut out, &mut err).unwrap(),
                0
            );
            let mut text = if source == Source::Bmclapi {
                b"# source\tBMCLAPI\n".to_vec()
            } else {
                vec![]
            };
            text.extend_from_slice(b"version\t");
            text.extend_from_slice(parsed.id());
            text.extend_from_slice(b"\ntarget\tlinux\tx86_64\nfiles\t1\nclasspath\t1\nnativeExtractions\t0\nassetProjections\t0\ndownloadBytes\t1\n");
            assert_eq!(out, text);
            // Exercise the raw decoder plus same injected RPC core, including ignored overflow.
            let input = raw(&format!(
                r#"{{"id":{},"source":"{}","ignored":1e9999}}"#,
                id,
                source.name()
            ));
            assert!(selected(Some(&input)));
            rpc(&svc, prepare(Some(&input)).unwrap()).unwrap();
            assert!(svc.transport.calls.borrow().is_empty());
            assert_eq!(snapshot(owner.path()), before);
        }
    }
}
#[test]
fn cached_lookup_parse_identity_errors_and_diagnostics_stay_ordered() {
    for (case, error) in [
        (0, Error::VersionNotFound),
        (1, Error::MetadataIdMismatch),
        (2, Error::Json("SyntaxError")),
    ] {
        let (owner, svc) = fixture(vec![]);
        let bytes = if case == 2 {
            b"{".to_vec()
        } else {
            body(json!([253]), None)
        };
        setup(&svc, Source::Official, json!([255]), &bytes);
        let before = snapshot(owner.path());
        let p = json!({"id":if case==0{json!([252])}else{json!([255])}});
        assert_eq!(
            invoke(&svc, &p).unwrap_err(),
            map_error("install.plan", error.clone())
        );
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            cli_execute(&svc, "install.plan", &p, false, &mut out, &mut err).unwrap(),
            1
        );
        assert!(out.is_empty());
        assert_eq!(
            err,
            format!(
                "failed to create install plan from {}: {}\n",
                Source::Official.display(),
                error.name()
            )
            .as_bytes()
        );
        assert!(svc.transport.calls.borrow().is_empty());
        assert_eq!(snapshot(owner.path()), before);
    }
}
#[test]
fn asset_hash_parse_and_late_component_validation_use_only_injected_json() {
    let good = br#"{"objects":{}}"#.to_vec();
    for case in 0..5 {
        let malformed = b"{".to_vec();
        let expected = if case == 2 { &malformed } else { &good };
        let reply = if case == 1 {
            malformed.clone()
        } else {
            expected.clone()
        };
        let id = if case >= 3 {
            json!([255, 47])
        } else {
            json!([255])
        };
        let result = if case == 3 {
            Err(Error::Network)
        } else {
            Ok(reply)
        };
        let (owner, svc) = fixture(vec![asset_reply(Source::Official, result)]);
        setup(
            &svc,
            Source::Official,
            id.clone(),
            &body(id.clone(), Some(expected)),
        );
        let before = snapshot(owner.path());
        let response = invoke(&svc, &json!({"id":id}));
        match case {
            0 => {
                assert_eq!(
                    response.unwrap().result["files"].as_array().unwrap().len(),
                    2
                );
            }
            1 => assert_eq!(
                response.unwrap_err(),
                map_error("install.plan", Error::AssetIndexHashMismatch)
            ),
            2 => assert_eq!(
                response.unwrap_err(),
                map_error("install.plan", Error::Json("SyntaxError"))
            ),
            3 => assert_eq!(
                response.unwrap_err(),
                map_error("install.plan", Error::Network)
            ),
            _ => assert_eq!(
                response.unwrap_err(),
                map_error("install.plan", Error::Plan("UnsafePath"))
            ),
        }
        assert_eq!(svc.transport.calls.borrow().len(), 1);
        assert!(svc.transport.replies.borrow().is_empty());
        assert_eq!(snapshot(owner.path()), before);
    }
}
#[test]
fn bmclapi_retries_whole_attempt_and_retains_requested_download_source() {
    let asset = br#"{"objects":{}}"#.to_vec();
    for final_failure in [false, true] {
        let (owner, svc) = fixture(vec![
            asset_reply(Source::Bmclapi, Err(Error::Network)),
            asset_reply(
                Source::Official,
                if final_failure {
                    Err(Error::UnsuccessfulHttpStatus)
                } else {
                    Ok(asset.clone())
                },
            ),
        ]);
        let bytes = body(json!([255]), Some(&asset));
        for s in [Source::Bmclapi, Source::Official] {
            setup(&svc, s, json!([255]), &bytes);
        }
        let before = snapshot(owner.path());
        let result = invoke(&svc, &json!({"id":[255],"source":"bmclapi"}));
        if final_failure {
            assert_eq!(
                result.unwrap_err(),
                map_error("install.plan", Error::UnsuccessfulHttpStatus)
            );
        } else {
            let r = result.unwrap();
            assert_eq!(r.source, Source::Bmclapi);
            assert_eq!(r.result["source"], "bmclapi");
            assert_eq!(
                r.result["files"][0]["url"],
                "https://bmclapi2.bangbang93.com/never-fetched"
            );
        }
        assert_eq!(svc.transport.calls.borrow().len(), 2);
        assert!(svc.transport.replies.borrow().is_empty());
        assert_eq!(snapshot(owner.path()), before);
    }
    // A pure plan error also retries; no eager request-path guard is substituted.
    let (owner, svc) = fixture(vec![]);
    let mut v: Value = serde_json::from_slice(&body(json!([255]), None)).unwrap();
    v["downloads"]["client"]["url"] = "http://unsafe".into();
    setup(
        &svc,
        Source::Bmclapi,
        json!([255]),
        &serde_json::to_vec(&v).unwrap(),
    );
    setup(
        &svc,
        Source::Official,
        json!([255]),
        &body(json!([255]), None),
    );
    let before = snapshot(owner.path());
    assert!(invoke(&svc, &json!({"id":[255],"source":"bmclapi"})).is_ok());
    assert_eq!(snapshot(owner.path()), before);
}
#[test]
fn cached_hash_retry_changes_exactly_one_entry_and_preserves_canaries() {
    let bytes = body(json!([255]), None);
    let (owner, svc) = fixture(vec![(META.into(), 4 * 1024 * 1024, Ok(bytes.clone()))]);
    setup(&svc, Source::Official, json!([255]), &bytes);
    seed(&svc, META, b"{");
    let mut before = snapshot(owner.path());
    invoke(&svc, &json!({"id":[255]})).unwrap();
    let mut after = snapshot(owner.path());
    let relative = svc
        .cache_path(META)
        .strip_prefix(owner.path())
        .unwrap()
        .to_path_buf();
    assert_ne!(
        before.remove(&relative).unwrap(),
        after.remove(&relative).unwrap()
    );
    let cache = std::path::PathBuf::from("cache");
    let old = before.get(&cache).unwrap();
    let new = after.get_mut(&cache).unwrap();
    new.5 = old.5;
    new.6 = old.6;
    assert_eq!(after, before);
    let env: Envelope = serde_json::from_slice(&fs::read(svc.cache_path(META)).unwrap()).unwrap();
    assert_eq!(env.url, META);
    assert_eq!(env.payload.bytes(), bytes);
    assert_eq!(svc.transport.calls.borrow().len(), 1);
    assert!(svc.transport.replies.borrow().is_empty());
    let stable = snapshot(owner.path());
    invoke(&svc, &json!({"id":[255]})).unwrap();
    assert_eq!(snapshot(owner.path()), stable);
    assert_eq!(svc.transport.calls.borrow().len(), 1);
}

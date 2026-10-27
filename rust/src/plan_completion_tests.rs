use super::*;
use crate::{metadata, version};
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{
    cell::RefCell, collections::VecDeque, fs, os::unix::fs::MetadataExt, path::PathBuf, sync::Mutex,
};
const DATA: &[u8] = b"opaque synthetic content";
const FILE: &str = "https://fixture.invalid/file";
const META: &str = "https://fixture.invalid/metadata";
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha1::digest(b))
}
fn target() -> Target {
    Target {
        os: "linux".into(),
        arch: "x86_64".into(),
        os_version: None,
    }
}
fn raw(id: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":FILE,"sha1":hash(DATA),"size":DATA.len()}}})).unwrap()
}
struct Metadata {
    replies: RefCell<VecDeque<(String, Vec<u8>)>>,
    calls: RefCell<Vec<String>>,
}
impl version::Transport for Metadata {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, metadata::Error> {
        assert_eq!(
            max,
            if url == META {
                4 * 1024 * 1024
            } else {
                2 * 1024 * 1024
            }
        );
        self.calls.borrow_mut().push(url.into());
        let (u, b) = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected metadata request");
        assert_eq!(u, url);
        Ok(b)
    }
}
struct Files {
    replies: Mutex<VecDeque<Vec<u8>>>,
    calls: Mutex<usize>,
}
impl Files {
    fn new(data: Vec<Vec<u8>>) -> Self {
        Self {
            replies: Mutex::new(data.into()),
            calls: Mutex::new(0),
        }
    }
}
impl install::Transport for Files {
    fn fetch(&self, url: &str, out: &mut dyn Write) -> Result<u16, install::Error> {
        assert_eq!(url, FILE);
        *self.calls.lock().unwrap() += 1;
        out.write_all(
            &self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected file request"),
        )?;
        Ok(200)
    }
}
type Snap = (Vec<u8>, u64, u64, u64, u32, i64, i64);
fn snap(p: &Path) -> Snap {
    let m = fs::metadata(p).unwrap();
    (
        fs::read(p).unwrap(),
        m.dev(),
        m.ino(),
        m.nlink(),
        m.mode(),
        m.mtime(),
        m.mtime_nsec(),
    )
}
struct Owned {
    owner: tempfile::TempDir,
    destination: PathBuf,
    cas: PathBuf,
    cache: PathBuf,
    canary: Snap,
}
impl Owned {
    fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        let destination = owner.path().join("instances/demo/.minecraft");
        let cas = owner.path().join("cas");
        let cache = owner.path().join("cache");
        assert!(crate::instances::validate_id("demo").is_ok());
        for p in [&destination, &cas, &cache] {
            assert!(p.is_absolute() && !p.as_os_str().is_empty() && p.starts_with(owner.path()));
        }
        fs::write(owner.path().join("sentinel"), b"external canary").unwrap();
        fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
        let mut perms = fs::metadata(owner.path().join("sentinel"))
            .unwrap()
            .permissions();
        perms.set_readonly(true);
        fs::set_permissions(owner.path().join("sentinel"), perms).unwrap();
        let canary = snap(&owner.path().join("sentinel"));
        Self {
            owner,
            destination,
            cas,
            cache,
            canary,
        }
    }
    fn check(&self) {
        assert_eq!(snap(&self.owner.path().join("sentinel")), self.canary);
        assert_eq!(snap(&self.owner.path().join("peer")), self.canary);
    }
    fn service(&self, id: Value, body: Vec<u8>) -> version::Service<Metadata> {
        let manifest=serde_json::to_vec(&json!({"latest":{"release":"v","snapshot":"v"},"versions":[{"id":id,"type":"release","url":META,"sha1":hash(&body),"time":"t","releaseTime":"t"}]})).unwrap();
        let mut svc = version::Service::new(
            Metadata {
                replies: RefCell::new(
                    vec![
                        (Source::Official.manifest_url().into(), manifest),
                        (META.into(), body),
                    ]
                    .into(),
                ),
                calls: RefCell::new(vec![]),
            },
            &self.cache,
        );
        svc.now = 1000;
        svc
    }
    fn receipt(&self) -> PathBuf {
        self.destination.join(".jmcl/install.json")
    }
    fn old_receipt(&self) -> Snap {
        fs::create_dir_all(self.receipt().parent().unwrap()).unwrap();
        fs::write(self.receipt(), b"previous receipt").unwrap();
        fs::hard_link(self.receipt(), self.owner.path().join("receipt-peer")).unwrap();
        snap(&self.receipt())
    }
}
fn planned(o: &Owned, id: Value) -> ReadPlan {
    let bytes = match &id {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::Array(a) => a.iter().map(|v| v.as_u64().unwrap() as u8).collect(),
        _ => panic!(),
    };
    let svc = o.service(id.clone(), raw(id));
    let p = svc
        .vanilla_read_plan(&bytes, Source::Official, target())
        .unwrap();
    assert_eq!(svc.transport.calls.borrow().len(), 2);
    p
}
fn check_files(o: &Owned, p: &ReadPlan, links: u64) -> (Snap, Snap) {
    let dest = o
        .destination
        .join(crate::store::path_from_bytes(&p.files[0].path.0).unwrap());
    let object = o.cas.join(crate::store::object_path(&hash(DATA)).unwrap());
    let a = snap(&dest);
    let b = snap(&object);
    assert_eq!(a, b);
    assert_eq!(a.0, DATA);
    assert_eq!(a.3, links);
    assert_eq!(a.4 & 0o222, 0);
    assert!(
        o.cas
            .join(format!("locks/sha1/{}.lock", hash(DATA)))
            .is_file()
    );
    o.check();
    (a, b)
}
fn lines(out: &[u8]) -> Vec<Value> {
    std::str::from_utf8(out)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn compare_utf8_completion(deferred: bool) {
    let mut outcomes = vec![];
    for byte in [false, true] {
        let o = Owned::new();
        let mut p = planned(&o, json!("版本"));
        if deferred {
            p.native_extractions.push(NativeExtraction {
                archive_path: String::from_utf8(p.files[0].path.0.clone()).unwrap(),
                exclude: vec![],
            });
            p.asset_projections.push(AssetProjection {
                source_path: String::from_utf8(p.files[0].path.0.clone()).unwrap(),
                destination_path: "resources/deferred".into(),
                kind: "resources".into(),
            });
        }
        let t = Files::new(vec![DATA.to_vec()]);
        let mut out = vec![];
        if byte {
            if deferred {
                p.vanilla_download_phase()
                    .unwrap()
                    .complete(
                        "test",
                        o.destination.as_os_str().as_encoded_bytes(),
                        o.cas.as_os_str().as_encoded_bytes(),
                        install::Options {
                            workers: 1,
                            retries: 0,
                            sync_files: true,
                        },
                        &t,
                        &mut out,
                    )
                    .unwrap();
            } else {
                p.complete_ordinary("test", &o.destination, &o.cas, &t, &mut out)
                    .unwrap()
                    .unwrap();
            }
        } else {
            // Existing injected String sequence, without invoking the full handler.
            let mut public = crate::plan::build(
                &metadata::metadata(&raw(json!("版本"))).unwrap(),
                None,
                Source::Official,
                target(),
            )
            .unwrap();
            public.native_extractions = p.native_extractions.clone();
            public.asset_projections = p.asset_projections.clone();
            let root = ArtifactRoot::open(&o.destination).unwrap();
            let store = Store::open(&o.cas).unwrap();
            writeln!(out,"{{\"protocol\":1,\"id\":\"test\",\"event\":\"started\",\"source\":\"official\",\"started\":{{\"stage\":\"download\",\"version_id\":\"版本\",\"files_total\":1,\"bytes_total\":{}}}}}",DATA.len()).unwrap();
            let summary = install::stream::stream(
                |sink| {
                    install::execute(
                        &root,
                        &store,
                        &public,
                        install::Options {
                            workers: 1,
                            retries: 0,
                            sync_files: true,
                        },
                        &t,
                        Some(sink),
                    )
                },
                |e| install::rpc::write_event(&mut out, "test", Source::Official, e),
            )
            .unwrap()
            .unwrap();
            install::service::record_completed(
                &root,
                &install::service::Selection {
                    id: "版本".into(),
                    fabric_loader: None,
                    neoforge_version: None,
                    forge_version: None,
                    source: Source::Official,
                    os: None,
                    arch: None,
                },
                &target(),
            )
            .unwrap();
            let summary = serde_json::to_string(&summary).unwrap();
            writeln!(out,"{{\"protocol\":1,\"id\":\"test\",\"event\":\"result\",\"source\":\"official\",\"result\":{{\"stage\":\"download\",\"version_id\":\"版本\",{}}}}}",&summary[1..summary.len()-1]).unwrap();
        }
        check_files(&o, &p, 2);
        assert_eq!(*t.calls.lock().unwrap(), 1);
        outcomes.push((out, fs::read(o.receipt()).unwrap()));
    }
    assert_eq!(outcomes[0], outcomes[1]);
}
#[test]
fn utf8_completion_matches_existing_string_sequence_exact_output() {
    compare_utf8_completion(false);
}
#[test]
fn opaque_completion_repeat_preserves_file_peers_and_visible_locks() {
    let o = Owned::new();
    let p = planned(&o, json!([255]));
    let t = Files::new(vec![DATA.to_vec()]);
    let mut out = vec![];
    p.complete_ordinary("test", &o.destination, &o.cas, &t, &mut out)
        .unwrap();
    check_files(&o, &p, 2);
    let dest = o
        .destination
        .join(crate::store::path_from_bytes(&p.files[0].path.0).unwrap());
    fs::hard_link(&dest, o.owner.path().join("file-peer")).unwrap();
    let before = check_files(&o, &p, 3);
    let peer = snap(&o.owner.path().join("file-peer"));
    let receipt = fs::read(o.receipt()).unwrap();
    let r: Value = serde_json::from_slice(&receipt).unwrap();
    assert_eq!(r["version_id"], json!([255]));
    out.clear();
    let summary = p
        .complete_ordinary("test", &o.destination, &o.cas, &t, &mut out)
        .unwrap()
        .unwrap();
    assert_eq!(summary.files_cached, 1);
    assert_eq!(*t.calls.lock().unwrap(), 1);
    assert_eq!(check_files(&o, &p, 3), before);
    assert_eq!(snap(&o.owner.path().join("file-peer")), peer);
    assert_eq!(fs::read(o.receipt()).unwrap(), receipt);
    let events = lines(&out);
    assert_eq!(
        events
            .iter()
            .map(|v| v["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["started", "file", "progress", "result"]
    );
    assert_eq!(events[0]["started"]["version_id"], json!([255]));
    assert_eq!(events[1]["file"]["status"], "cached");
}
#[test]
fn metadata_failure_precedes_destination_and_cas() {
    let o = Owned::new();
    let svc = o.service(json!("wanted"), raw(json!("different")));
    assert!(
        svc.vanilla_read_plan(b"wanted", Source::Official, target())
            .is_err()
    );
    assert!(!o.destination.exists());
    assert!(!o.cas.exists());
    assert_eq!(svc.transport.calls.borrow().len(), 2);
    o.check();
}
#[test]
fn file_failures_preserve_prior_receipt_and_cleanup_staging() {
    for data in [b"short".to_vec(), vec![b'x'; DATA.len()]] {
        let o = Owned::new();
        let p = planned(&o, json!("v"));
        let old = o.old_receipt();
        let t = Files::new(vec![data]);
        let mut out = vec![];
        let e = p
            .complete_ordinary("test", &o.destination, &o.cas, &t, &mut out)
            .unwrap_err();
        assert!(matches!(e, Failure::Files(_)));
        assert_eq!(snap(&o.receipt()), old);
        assert_eq!(snap(&o.owner.path().join("receipt-peer")), old);
        assert_eq!(*t.calls.lock().unwrap(), 1);
        assert!(
            !o.cas
                .join(crate::store::object_path(&hash(DATA)).unwrap())
                .exists()
        );
        assert!(
            o.cas
                .join(format!("locks/sha1/{}.lock", hash(DATA)))
                .is_file()
        );
        assert_eq!(lines(&out).len(), 1);
        o.check();
        fn scan(p: &Path) {
            for e in fs::read_dir(p).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    scan(&p)
                } else {
                    assert!(
                        p.extension().is_some_and(|x| x == "lock"),
                        "unexpected CAS file {p:?}"
                    );
                }
            }
        }
        scan(&o.cas);
    }
}
struct Failing {
    bytes: Vec<u8>,
    fail_event: &'static str,
    pending: Vec<u8>,
}
impl Write for Failing {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(b);
        if self.pending.ends_with(b"\n") {
            let v: Value = serde_json::from_slice(&self.pending).unwrap();
            if v["event"] == self.fail_event {
                return Err(io::Error::other("injected writer failure"));
            }
            self.bytes.append(&mut self.pending);
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn event_failure_prevents_receipt_but_final_output_failure_keeps_it() {
    for stage in ["started", "file", "progress", "result"] {
        let o = Owned::new();
        let p = planned(&o, json!("v"));
        let old = o.old_receipt();
        let t = Files::new(vec![DATA.to_vec()]);
        let mut out = Failing {
            bytes: vec![],
            pending: vec![],
            fail_event: stage,
        };
        assert!(matches!(
            p.complete_ordinary("test", &o.destination, &o.cas, &t, &mut out),
            Err(Failure::Output(_))
        ));
        if stage == "result" {
            assert_ne!(fs::read(o.receipt()).unwrap(), old.0);
            let mut peer = old.clone();
            peer.3 = 1;
            assert_eq!(snap(&o.owner.path().join("receipt-peer")), peer);
        } else {
            assert_eq!(snap(&o.receipt()), old);
            assert_eq!(snap(&o.owner.path().join("receipt-peer")), old);
        }
        if stage == "started" {
            assert_eq!(*t.calls.lock().unwrap(), 0);
        } else {
            check_files(&o, &p, 2);
            assert_eq!(*t.calls.lock().unwrap(), 1);
        }
        o.check();
    }
}
#[test]
fn receipt_failure_retains_completed_files_and_no_success_result() {
    let o = Owned::new();
    let p = planned(&o, json!("v"));
    fs::create_dir_all(o.receipt()).unwrap();
    fs::write(o.receipt().join("sentinel"), b"not a receipt").unwrap();
    let old = snap(&o.receipt().join("sentinel"));
    let t = Files::new(vec![DATA.to_vec()]);
    let mut out = vec![];
    assert!(matches!(
        p.complete_ordinary("test", &o.destination, &o.cas, &t, &mut out),
        Err(Failure::Receipt(_))
    ));
    assert_eq!(snap(&o.receipt().join("sentinel")), old);
    check_files(&o, &p, 2);
    assert_eq!(
        fs::read_dir(o.receipt().parent().unwrap()).unwrap().count(),
        1
    );
    assert!(!lines(&out).iter().any(|v| v["event"] == "result"));
}
#[test]
fn unsupported_capabilities_and_root_order_do_not_enter_execution() {
    let o = Owned::new();
    let p = planned(&o, json!("v"));
    let t = Files::new(vec![]);
    for case in 0..3 {
        let mut p = p.clone();
        match case {
            0 => p.loader = "forge".into(),
            1 => p.native_extractions.push(NativeExtraction {
                archive_path: "never".into(),
                exclude: vec![],
            }),
            _ => p.asset_projections.push(AssetProjection {
                source_path: "never".into(),
                destination_path: "never".into(),
                kind: "resources".into(),
            }),
        };
        let mut out = vec![];
        assert!(
            p.complete_ordinary("test", &o.destination, &o.cas, &t, &mut out)
                .unwrap()
                .is_none()
        );
        assert!(out.is_empty());
        assert!(!o.destination.exists());
        assert!(!o.cas.exists());
    }
    let mut out = vec![];
    assert!(matches!(
        p.complete_ordinary("test", Path::new(""), &o.cas, &t, &mut out),
        Err(Failure::Root(_))
    ));
    assert!(!o.cas.exists());
    assert!(matches!(
        p.complete_ordinary("test", &o.destination, Path::new(""), &t, &mut out),
        Err(Failure::Store(_))
    ));
    assert!(o.destination.exists());
    assert!(!o.receipt().exists());
    assert!(out.is_empty());
    assert_eq!(*t.calls.lock().unwrap(), 0);
    o.check();
}

fn rpc_raw(o: &Owned, id: Value) -> Box<serde_json::value::RawValue> {
    // All effectful invocations have explicit owned roots; defaults are pure-only.
    assert!(
        o.destination.starts_with(o.owner.path())
            && o.cas.starts_with(o.owner.path())
            && o.cache.starts_with(o.owner.path())
    );
    serde_json::value::RawValue::from_string(json!({"id":id,"directory":o.destination,"store_directory":o.cas,"workers":1,"retries":0,"os":"linux","arch":"x86_64","java_paths":[[255]],"java_policy":"managed"}).to_string()).unwrap()
}
#[test]
fn vanilla_phase_retains_deferred_descriptions_and_string_file_behavior() {
    compare_utf8_completion(true);
    let o = Owned::new();
    let mut p = planned(&o, json!("v"));
    p.native_extractions.push(NativeExtraction {
        archive_path: String::from_utf8(p.files[0].path.0.clone()).unwrap(),
        exclude: vec!["META-INF/".into()],
    });
    p.asset_projections.push(AssetProjection {
        source_path: String::from_utf8(p.files[0].path.0.clone()).unwrap(),
        destination_path: "resources/deferred".into(),
        kind: "resources".into(),
    });
    let before = serde_json::to_vec(&p).unwrap();
    assert!(p.ordinary_files().is_none());
    let t = Files::new(vec![DATA.to_vec()]);
    let mut output = vec![];
    p.vanilla_download_phase()
        .unwrap()
        .complete(
            "test",
            o.destination.as_os_str().as_encoded_bytes(),
            o.cas.as_os_str().as_encoded_bytes(),
            install::Options {
                workers: 1,
                retries: 0,
                sync_files: true,
            },
            &t,
            &mut output,
        )
        .unwrap();
    assert_eq!(serde_json::to_vec(&p).unwrap(), before);
    assert!(!o.destination.join("resources").exists());
    assert!(!o.destination.join("natives").exists());
    check_files(&o, &p, 2);
}
#[test]
fn injected_execute_rpc_opaque_cached_metadata_and_native_descriptions_do_not_prepare() {
    let o = Owned::new();
    let mut body: Value = serde_json::from_slice(&raw(json!([255]))).unwrap();
    body["libraries"] = json!([{"name":"fixture:opaque:1","natives":{"linux":"natives-linux"},"downloads":{"artifact":{"path":"fixture/ordinary.bin","url":FILE,"sha1":hash(DATA),"size":DATA.len()},"classifiers":{"natives-linux":{"path":"fixture/native.bin","url":FILE,"sha1":hash(DATA),"size":DATA.len()}}},"extract":{"exclude":["META-INF/"]}}]);
    let svc = o.service(json!([255]), serde_json::to_vec(&body).unwrap());
    let t = Files::new(vec![DATA.to_vec()]);
    let req = rpc_raw(&o, json!([255]));
    let mut out = vec![];
    crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t)
        .unwrap()
        .unwrap();
    let events = lines(&out);
    assert_eq!(events.last().unwrap()["event"], "result");
    assert_eq!(events.last().unwrap()["result"]["files_total"], 2);
    assert_eq!(events.last().unwrap()["result"]["version_id"], json!([255]));
    assert_eq!(svc.transport.calls.borrow().len(), 2);
    assert_eq!(*t.calls.lock().unwrap(), 1);
    assert!(!o.destination.join("natives").exists());
    let receipt: Value = serde_json::from_slice(&fs::read(o.receipt()).unwrap()).unwrap();
    assert_eq!(receipt["version_id"], json!([255]));
    let before = snap(&o.destination.join("libraries/fixture/native.bin"));
    out.clear();
    crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t)
        .unwrap()
        .unwrap();
    assert_eq!(svc.transport.calls.borrow().len(), 2);
    assert_eq!(*t.calls.lock().unwrap(), 1);
    assert_eq!(
        snap(&o.destination.join("libraries/fixture/native.bin")),
        before
    );
    o.check();
}
#[test]
fn injected_execute_planning_precedes_root_conversion_and_destination_precedes_cas() {
    let o = Owned::new();
    let svc = o.service(json!("v"), raw(json!("wrong")));
    let t = Files::new(vec![]);
    let mut params: Value = serde_json::from_str(rpc_raw(&o, json!("v")).get()).unwrap();
    params["directory"] = json!([0]);
    let req = serde_json::value::RawValue::from_string(params.to_string()).unwrap();
    let mut out = vec![];
    crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t)
        .unwrap()
        .unwrap();
    assert_eq!(lines(&out)[0]["error"]["code"], "INSTALL_PLAN_ERROR");
    assert!(!o.destination.exists());
    assert!(!o.cas.exists());
    let o = Owned::new();
    let svc = o.service(json!("v"), raw(json!("v")));
    fs::write(&o.cas, b"blocked store").unwrap();
    let req = rpc_raw(&o, json!("v"));
    out.clear();
    crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t)
        .unwrap()
        .unwrap();
    assert_eq!(lines(&out)[0]["error"]["code"], "STORE_ROOT_ERROR");
    assert!(o.destination.exists());
    assert_eq!(fs::read(&o.cas).unwrap(), b"blocked store");
    assert!(!o.receipt().exists());
    assert_eq!(*t.calls.lock().unwrap(), 0);
    o.check();
}
struct ChangeManifest<'a> {
    files: Files,
    path: &'a Path,
}
impl install::Transport for ChangeManifest<'_> {
    fn fetch(&self, url: &str, out: &mut dyn Write) -> Result<u16, install::Error> {
        fs::write(
            self.path,
            br#"{"id":"demo","name":"changed","version_id":"after"}"#,
        )?;
        self.files.fetch(url, out)
    }
}
#[test]
fn injected_execute_keeps_captured_selection_when_owned_manifest_changes() {
    let o = Owned::new();
    let manifest = o.owner.path().join("instance.json");
    fs::write(
        &manifest,
        br#"{"id":"demo","name":"before","version_id":"v"}"#,
    )
    .unwrap();
    let captured: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    let svc = o.service(
        captured["version_id"].clone(),
        raw(captured["version_id"].clone()),
    );
    let t = ChangeManifest {
        files: Files::new(vec![DATA.to_vec()]),
        path: &manifest,
    };
    let req = rpc_raw(&o, captured["version_id"].clone());
    let mut out = vec![];
    crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t)
        .unwrap()
        .unwrap();
    assert_eq!(lines(&out).last().unwrap()["event"], "result");
    let r: Value = serde_json::from_slice(&fs::read(o.receipt()).unwrap()).unwrap();
    assert_eq!(r["version_id"], "v");
    let changed: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(changed["version_id"], "after");
    o.check();
    // Demonstrates captured standalone selection only; no managed wrapper integration claim.
}
#[test]
fn injected_execute_failure_mapping_preserves_receipts_and_peer_identity() {
    for stage in ["file", "event", "receipt", "result"] {
        let o = Owned::new();
        let svc = o.service(json!("v"), raw(json!("v")));
        let req = rpc_raw(&o, json!("v"));
        let old = if stage == "receipt" {
            fs::create_dir_all(o.receipt()).unwrap();
            None
        } else {
            Some(o.old_receipt())
        };
        let t = Files::new(vec![if stage == "file" {
            b"short".to_vec()
        } else {
            DATA.to_vec()
        }]);
        let mut out = Failing {
            bytes: vec![],
            pending: vec![],
            fail_event: if stage == "event" {
                "file"
            } else if stage == "result" {
                "result"
            } else {
                "never"
            },
        };
        let result =
            crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t).unwrap();
        if matches!(stage, "event" | "result") {
            assert!(result.is_err())
        } else {
            result.unwrap();
            assert_eq!(
                lines(&out.bytes).last().unwrap()["error"]["code"],
                if stage == "file" {
                    "DOWNLOAD_FAILED"
                } else {
                    "INSTALL_STATE_ERROR"
                }
            );
        }
        if let Some(mut old) = old {
            if stage == "result" {
                old.3 = 1;
                assert_ne!(fs::read(o.receipt()).unwrap(), old.0);
            } else {
                assert_eq!(snap(&o.receipt()), old);
            }
            assert_eq!(snap(&o.owner.path().join("receipt-peer")), old);
        }
        assert!(
            o.cas
                .join(format!("locks/sha1/{}.lock", hash(DATA)))
                .exists()
        );
        o.check();
    }
}

#[test]
fn injected_execute_assets_fetched_once_for_planning_and_deferred_projections_retained() {
    const ASSET: &str = "https://fixture.invalid/assets";
    struct Meta {
        queue: RefCell<VecDeque<(String, usize, Vec<u8>)>>,
        calls: RefCell<Vec<String>>,
    }
    impl version::Transport for Meta {
        fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, metadata::Error> {
            self.calls.borrow_mut().push(url.into());
            let (u, n, b) = self
                .queue
                .borrow_mut()
                .pop_front()
                .expect("unexpected metadata request");
            assert_eq!((url, max), (u.as_str(), n));
            Ok(b)
        }
    }
    struct FileQueue {
        queue: Mutex<VecDeque<(String, Vec<u8>)>>,
        calls: Mutex<Vec<String>>,
    }
    impl install::Transport for FileQueue {
        fn fetch(&self, url: &str, out: &mut dyn Write) -> Result<u16, install::Error> {
            self.calls.lock().unwrap().push(url.into());
            let (u, b) = self
                .queue
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected artifact request");
            assert_eq!(url, u);
            out.write_all(&b)?;
            Ok(200)
        }
    }
    let o = Owned::new();
    let assets=serde_json::to_vec(&json!({"objects":{"fixture/item":{"hash":hash(DATA),"size":DATA.len()}},"virtual":true,"map_to_resources":true})).unwrap();
    let mut body: Value = serde_json::from_slice(&raw(json!("v"))).unwrap();
    body["assetIndex"] = json!({"id":"fixture","url":ASSET,"sha1":hash(&assets),"size":assets.len(),"totalSize":DATA.len()});
    let body = serde_json::to_vec(&body).unwrap();
    let manifest=serde_json::to_vec(&json!({"latest":{"release":"v","snapshot":"v"},"versions":[{"id":"v","type":"release","url":META,"sha1":hash(&body),"time":"t","releaseTime":"t"}]})).unwrap();
    let mut svc = version::Service::new(
        Meta {
            queue: RefCell::new(
                vec![
                    (
                        Source::Official.manifest_url().into(),
                        2 * 1024 * 1024,
                        manifest,
                    ),
                    (META.into(), 4 * 1024 * 1024, body),
                    (ASSET.into(), 16 * 1024 * 1024, assets.clone()),
                ]
                .into(),
            ),
            calls: RefCell::new(vec![]),
        },
        &o.cache,
    );
    svc.now = 1000;
    let t = FileQueue {
        queue: Mutex::new(
            vec![(FILE.into(), DATA.to_vec()), (ASSET.into(), assets.clone())].into(),
        ),
        calls: Mutex::new(vec![]),
    };
    let req = rpc_raw(&o, json!("v"));
    let mut out = vec![];
    crate::install::vanilla::handle_with(&mut out, "test", Some(&req), &svc, &t)
        .unwrap()
        .unwrap();
    let events = lines(&out);
    assert_eq!(events.last().unwrap()["event"], "result");
    assert_eq!(events.last().unwrap()["result"]["files_total"], 3);
    assert_eq!(
        svc.transport.calls.borrow().as_slice(),
        [Source::Official.manifest_url(), META, ASSET]
    );
    assert_eq!(t.calls.lock().unwrap().as_slice(), [FILE, ASSET]);
    assert!(svc.transport.queue.borrow().is_empty());
    assert!(t.queue.lock().unwrap().is_empty());
    assert!(!o.destination.join("resources").exists());
    assert!(!o.destination.join("assets/virtual").exists());
    assert_eq!(
        fs::read(o.destination.join("assets/indexes/fixture.json")).unwrap(),
        assets
    );
    let client = snap(&o.destination.join("versions/v/v.jar"));
    let asset = snap(&o.destination.join(format!(
        "assets/objects/{}/{}",
        &hash(DATA)[..2],
        hash(DATA)
    )));
    assert_eq!(client, asset);
    assert_eq!(client.3, 3);
    assert_eq!(client.4 & 0o222, 0);
    o.check();
}

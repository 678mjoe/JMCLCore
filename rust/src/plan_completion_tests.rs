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
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, metadata::Error> {
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
#[test]
fn utf8_completion_matches_existing_string_sequence_exact_output() {
    let mut outcomes = vec![];
    for byte in [false, true] {
        let o = Owned::new();
        let p = planned(&o, json!("版本"));
        let t = Files::new(vec![DATA.to_vec()]);
        let mut out = vec![];
        if byte {
            p.complete_ordinary("test", &o.destination, &o.cas, &t, &mut out)
                .unwrap()
                .unwrap();
        } else {
            // Existing injected String sequence, without invoking the full handler.
            let public = crate::plan::build(
                &metadata::metadata(&raw(json!("版本"))).unwrap(),
                None,
                Source::Official,
                target(),
            )
            .unwrap();
            let root = ArtifactRoot::open(&o.destination).unwrap();
            let store = Store::open(&o.cas).unwrap();
            write!(out,"{{\"protocol\":1,\"id\":\"test\",\"event\":\"started\",\"source\":\"official\",\"payload\":{{\"started\":{{\"stage\":\"download\",\"version_id\":\"版本\",\"files_total\":1,\"bytes_total\":{}}}}}}}\n",DATA.len()).unwrap();
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
            writeln!(out,"{{\"protocol\":1,\"id\":\"test\",\"event\":\"result\",\"source\":\"official\",\"payload\":{{\"result\":{{\"stage\":\"download\",\"version_id\":\"版本\",{}}}}}}}",&summary[1..summary.len()-1]).unwrap();
        }
        check_files(&o, &p, 2);
        assert_eq!(*t.calls.lock().unwrap(), 1);
        outcomes.push((out, fs::read(o.receipt()).unwrap()));
    }
    assert_eq!(outcomes[0], outcomes[1]);
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
    assert_eq!(events[0]["payload"]["started"]["version_id"], json!([255]));
    assert_eq!(events[1]["payload"]["file"]["status"], "cached");
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
    for stage in ["started", "file", "result"] {
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
        Err(Failure::Root)
    ));
    assert!(!o.cas.exists());
    assert!(matches!(
        p.complete_ordinary("test", &o.destination, Path::new(""), &t, &mut out),
        Err(Failure::Store)
    ));
    assert!(o.destination.exists());
    assert!(!o.receipt().exists());
    assert!(out.is_empty());
    assert_eq!(*t.calls.lock().unwrap(), 0);
    o.check();
}

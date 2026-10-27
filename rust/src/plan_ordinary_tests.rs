//! Linux-only owned synthetic file fixtures. No archive, Java or full install RPC.
use super::*;
use crate::{
    install, metadata,
    store::{ArtifactRoot, Store},
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Mutex,
};
const PAYLOAD: &[u8] = b"opaque synthetic file data";
const URL: &str = "https://fixture.invalid/ordinary";
struct Fake {
    queue: Mutex<VecDeque<Vec<u8>>>,
    calls: Mutex<usize>,
}
impl Fake {
    fn new(payloads: Vec<Vec<u8>>) -> Self {
        Self {
            queue: Mutex::new(payloads.into()),
            calls: Mutex::new(0),
        }
    }
}
impl install::Transport for Fake {
    fn fetch(&self, url: &str, sink: &mut dyn std::io::Write) -> Result<u16, install::Error> {
        assert_eq!(url, URL);
        *self.calls.lock().unwrap() += 1;
        let data = self
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected transport");
        sink.write_all(&data)?;
        Ok(200)
    }
}
fn raw(id: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":URL,"sha1":format!("{:x}",Sha1::digest(PAYLOAD)),"size":PAYLOAD.len()}}})).unwrap()
}
fn plan(id: Value) -> ReadPlan {
    let m = metadata::ReadMetadata::parse(&raw(id)).unwrap();
    super::super::build_read(
        &m,
        None,
        metadata::Source::Official,
        Target {
            os: "linux".into(),
            arch: "x86_64".into(),
            os_version: None,
        },
    )
    .unwrap()
}
fn options() -> install::Options {
    install::Options {
        workers: 1,
        retries: 0,
        sync_files: true,
    }
}
type Snap = (Vec<u8>, u64, u64, u64, u32, i64, i64);
fn snap(path: &Path) -> Snap {
    let m = fs::symlink_metadata(path).unwrap();
    (
        if m.is_file() {
            fs::read(path).unwrap()
        } else {
            vec![]
        },
        m.dev(),
        m.ino(),
        m.nlink(),
        m.mode(),
        m.mtime(),
        m.mtime_nsec(),
    )
}
fn paths(root: &Path) -> BTreeSet<PathBuf> {
    fn walk(root: &Path, p: &Path, set: &mut BTreeSet<PathBuf>) {
        for e in fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            set.insert(p.strip_prefix(root).unwrap().into());
            if p.is_dir() {
                walk(root, &p, set)
            }
        }
    }
    let mut s = BTreeSet::new();
    walk(root, root, &mut s);
    s
}
struct Owned {
    owner: tempfile::TempDir,
    destination: PathBuf,
    cas: PathBuf,
    root: ArtifactRoot,
    store: Store,
    canaries: BTreeMap<PathBuf, Snap>,
}
impl Owned {
    fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        let destination = owner.path().join("destination");
        let cas = owner.path().join("cas");
        for p in [&destination, &cas] {
            assert!(!p.as_os_str().is_empty() && p.parent() == Some(owner.path()));
        }
        let root = ArtifactRoot::open(&destination).unwrap();
        let store = Store::open(&cas).unwrap();
        let sentinel = owner.path().join("sentinel");
        fs::write(&sentinel, b"sibling canary").unwrap();
        fs::hard_link(&sentinel, owner.path().join("sentinel-peer")).unwrap();
        let mut perms = fs::metadata(&sentinel).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&sentinel, perms).unwrap();
        let canaries = [sentinel, owner.path().join("sentinel-peer")]
            .into_iter()
            .map(|p| {
                let s = snap(&p);
                (p, s)
            })
            .collect();
        Self {
            owner,
            destination,
            cas,
            root,
            store,
            canaries,
        }
    }
    fn unchanged(&self) {
        for (p, s) in &self.canaries {
            assert_eq!(&snap(p), s);
        }
        assert!(!self.destination.join(".jmcl/install.json").exists());
    }
}
fn byte_event(e: install::ByteEvent) -> Value {
    match e {
        install::ByteEvent::File(v) => json!({"file":v}),
        install::ByteEvent::Retry(v) => json!({"retry":v}),
        install::ByteEvent::Progress(v) => json!({"progress":v}),
    }
}
fn string_event(e: install::Event) -> Value {
    match e {
        install::Event::File(v) => json!({"file":v}),
        install::Event::Retry(v) => json!({"retry":v}),
        install::Event::Progress(v) => json!({"progress":v}),
    }
}
fn expected(path: Value, cached: bool) -> Vec<Value> {
    vec![
        json!({"file":{"path":path,"status":if cached{"cached"}else{"downloaded"},"placement":"hard_link","size":PAYLOAD.len(),"attempts":if cached{0}else{1}}}),
        json!({"progress":{"files_completed":1,"files_total":1,"bytes_verified":PAYLOAD.len(),"bytes_processed":PAYLOAD.len(),"bytes_total":PAYLOAD.len(),"bytes_transferred":if cached{0}else{PAYLOAD.len()}}}),
    ]
}
fn check_shared(o: &Owned, p: &ReadPlan, links: u64) {
    let file = &p.files[0];
    let dest = o
        .destination
        .join(crate::store::path_from_bytes(&file.path.0).unwrap());
    let object = o.cas.join(crate::store::object_path(&file.sha1).unwrap());
    let d = snap(&dest);
    let c = snap(&object);
    assert_eq!(d.0, PAYLOAD);
    assert_eq!(c.0, PAYLOAD);
    assert_eq!((d.1, d.2, d.3), (c.1, c.2, c.3));
    assert_eq!(d.3, links);
    assert_eq!(d.4 & 0o222, 0);
    assert_eq!(c.4 & 0o222, 0);
    assert!(
        o.cas
            .join(format!("locks/sha1/{}.lock", file.sha1))
            .is_file()
    );
    o.unchanged();
}
#[test]
fn unsupported_plan_selection_is_pure_before_owned_roots() {
    for case in 0..3 {
        let mut p = plan(json!([255]));
        match case {
            0 => p.loader = "fabric".into(),
            1 => p.native_extractions.push(NativeExtraction {
                archive_path: "never-opened".into(),
                exclude: vec![],
            }),
            _ => p.asset_projections.push(AssetProjection {
                source_path: "a".into(),
                destination_path: "b".into(),
                kind: "resources".into(),
            }),
        };
        assert!(p.ordinary_files().is_none());
    }
    // No root/store/transport factory is called in this rejection test.
    assert!(plan(json!([255])).ordinary_files().is_some());
}
#[test]
fn ordinary_utf8_adapter_matches_string_engine_events_summary_and_files() {
    let mut outcomes = vec![];
    for bytes in [false, true] {
        let p = plan(json!("版本"));
        let selected = p.ordinary_files().unwrap();
        let o = Owned::new();
        let transport = Fake::new(vec![PAYLOAD.to_vec()]);
        let events = Mutex::new(vec![]);
        let result = if bytes {
            selected
                .execute(
                    &o.root,
                    &o.store,
                    options(),
                    &transport,
                    Some(&|e| {
                        events.lock().unwrap().push(byte_event(e));
                        Ok(())
                    }),
                )
                .unwrap()
        } else {
            let public = super::super::build(
                &metadata::metadata(&raw(json!("版本"))).unwrap(),
                None,
                metadata::Source::Official,
                p.target.clone(),
            )
            .unwrap();
            install::execute(
                &o.root,
                &o.store,
                &public,
                options(),
                &transport,
                Some(&|e| {
                    events.lock().unwrap().push(string_event(e));
                    Ok(())
                }),
            )
            .unwrap()
        };
        assert_eq!(
            result,
            install::Summary {
                files_total: 1,
                files_downloaded: 1,
                files_cached: 0,
                files_hard_linked: 1,
                files_copied: 0,
                bytes_verified: PAYLOAD.len() as u64,
                bytes_transferred: PAYLOAD.len() as u64
            }
        );
        let events = events.into_inner().unwrap();
        assert_eq!(events, expected(json!("versions/版本/版本.jar"), false));
        check_shared(&o, &p, 2);
        assert_eq!(*transport.calls.lock().unwrap(), 1);
        assert!(transport.queue.lock().unwrap().is_empty());
        eprintln!("retained CAS paths: {:?}", paths(&o.cas));
        outcomes.push((result, events));
    }
    assert_eq!(outcomes[0], outcomes[1]);
}
#[test]
fn opaque_version_files_repeat_without_transport_and_preserve_hardlink_peer() {
    let p = plan(json!([255, 128]));
    let selected = p.ordinary_files().unwrap();
    let o = Owned::new();
    let transport = Fake::new(vec![PAYLOAD.to_vec()]);
    let events = Mutex::new(vec![]);
    selected
        .execute(
            &o.root,
            &o.store,
            options(),
            &transport,
            Some(&|e| {
                events.lock().unwrap().push(byte_event(e));
                Ok(())
            }),
        )
        .unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        expected(json!(p.files[0].path.0), false)
    );
    check_shared(&o, &p, 2);
    let dest = o
        .destination
        .join(crate::store::path_from_bytes(&p.files[0].path.0).unwrap());
    let peer = o.owner.path().join("payload-peer");
    fs::hard_link(&dest, &peer).unwrap();
    let before = snap(&peer);
    let dest_before = snap(&dest);
    let all_before = paths(o.owner.path());
    events.lock().unwrap().clear();
    let result = selected
        .execute(
            &o.root,
            &o.store,
            options(),
            &transport,
            Some(&|e| {
                events.lock().unwrap().push(byte_event(e));
                Ok(())
            }),
        )
        .unwrap();
    assert_eq!(
        result,
        install::Summary {
            files_total: 1,
            files_downloaded: 0,
            files_cached: 1,
            files_hard_linked: 1,
            files_copied: 0,
            bytes_verified: PAYLOAD.len() as u64,
            bytes_transferred: 0
        }
    );
    assert_eq!(
        *events.lock().unwrap(),
        expected(json!(p.files[0].path.0), true)
    );
    check_shared(&o, &p, 3);
    assert_eq!(snap(&peer), before);
    assert_eq!(snap(&dest), dest_before);
    assert_eq!(paths(o.owner.path()), all_before);
    assert_eq!(*transport.calls.lock().unwrap(), 1);
    eprintln!("retained CAS paths: {:?}", paths(&o.cas));
}
#[test]
fn failed_payloads_leave_only_expected_lock_and_empty_prefix() {
    for (payload, error) in [
        (b"short".to_vec(), "DownloadSizeMismatch"),
        (vec![b'x'; PAYLOAD.len()], "DownloadHashMismatch"),
        (vec![b'x'; PAYLOAD.len() + 1], "ResponseTooLarge"),
    ] {
        let p = plan(json!([255]));
        let selected = p.ordinary_files().unwrap();
        let o = Owned::new();
        let before = paths(o.owner.path());
        let transport = Fake::new(vec![payload]);
        let events = Mutex::new(vec![]);
        let result = selected.execute(
            &o.root,
            &o.store,
            options(),
            &transport,
            Some(&|e| {
                events.lock().unwrap().push(byte_event(e));
                Ok(())
            }),
        );
        assert_eq!(result.unwrap_err().name(), error);
        assert!(events.lock().unwrap().is_empty());
        assert_eq!(*transport.calls.lock().unwrap(), 1);
        assert!(transport.queue.lock().unwrap().is_empty());
        assert!(paths(&o.destination).is_empty());
        let after = paths(o.owner.path());
        let hash = &p.files[0].sha1;
        assert_eq!(
            after.difference(&before).cloned().collect::<BTreeSet<_>>(),
            [
                PathBuf::from(format!("cas/locks/sha1/{hash}.lock")),
                PathBuf::from(format!("cas/objects/sha1/{}", &hash[..2]))
            ]
            .into_iter()
            .collect()
        );
        assert!(before.is_subset(&after));
        o.unchanged();
        eprintln!("{error} retained CAS paths: {:?}", paths(&o.cas));
    }
}

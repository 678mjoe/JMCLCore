use super::*;
use crate::{
    metadata,
    plan::{self, Target},
    store,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Mutex,
};
const DATA: &[u8] = b"owned synthetic data";
const FILE: &str = "https://fixture.invalid/file";
const META: &str = "https://fixture.invalid/meta";
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha1::digest(b))
}
fn body(id: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":FILE,"sha1":hash(DATA),"size":DATA.len()}}})).unwrap()
}
fn manifest(id: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id":"demo","name":"n","version_id":id,"source":"official"}))
        .unwrap()
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
    instances: PathBuf,
    cas: PathBuf,
    cache: PathBuf,
    manifest: PathBuf,
    destination: PathBuf,
    canary: Snap,
}
impl Owned {
    fn new(bytes: Option<&[u8]>) -> Self {
        let owner = tempfile::tempdir().unwrap();
        let instances = owner.path().join("instances");
        let cas = owner.path().join("cas");
        let cache = owner.path().join("cache");
        let path = instances.join("demo/instance.json");
        let destination = instances.join("demo/.minecraft");
        for p in [&instances, &cas, &cache] {
            assert!(p.is_absolute() && p.starts_with(owner.path()) && !p.as_os_str().is_empty());
        }
        assert!(instances::validate_id("demo").is_ok());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Some(b) = bytes {
            fs::write(&path, b).unwrap();
        }
        fs::write(owner.path().join("sentinel"), b"canary").unwrap();
        fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
        let mut perms = fs::metadata(owner.path().join("sentinel"))
            .unwrap()
            .permissions();
        perms.set_readonly(true);
        fs::set_permissions(owner.path().join("sentinel"), perms).unwrap();
        let canary = snap(&owner.path().join("sentinel"));
        Self {
            owner,
            instances,
            cas,
            cache,
            manifest: path,
            destination,
            canary,
        }
    }
    fn args(&self) -> Vec<String> {
        vec![
            "demo".into(),
            "--instances-directory".into(),
            self.instances.to_str().unwrap().into(),
            "--store-directory".into(),
            self.cas.to_str().unwrap().into(),
            "--workers".into(),
            "1".into(),
            "--retries".into(),
            "0".into(),
            "--os".into(),
            "linux".into(),
            "--arch".into(),
            "x86_64".into(),
        ]
    }
    fn check(&self) {
        assert_eq!(snap(&self.owner.path().join("sentinel")), self.canary);
        assert_eq!(snap(&self.owner.path().join("peer")), self.canary);
    }
    fn receipt(&self) -> PathBuf {
        self.destination.join(".jmcl/install.json")
    }
    fn previous(&self) -> Snap {
        fs::create_dir_all(self.receipt().parent().unwrap()).unwrap();
        fs::write(self.receipt(), b"previous receipt").unwrap();
        fs::hard_link(self.receipt(), self.owner.path().join("receipt-peer")).unwrap();
        let mut permissions = fs::metadata(self.receipt()).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(self.receipt(), permissions).unwrap();
        snap(&self.receipt())
    }
    fn service(&self, id: Value) -> version::Service<Meta> {
        let b = body(id.clone());
        let m=serde_json::to_vec(&json!({"latest":{"release":"v","snapshot":"v"},"versions":[{"id":id,"type":"release","url":META,"sha1":hash(&b),"time":"t","releaseTime":"t"}]})).unwrap();
        let mut svc = version::Service::new(
            Meta {
                queue: RefCell::new(
                    vec![
                        (Source::Official.manifest_url().into(), m),
                        (META.into(), b),
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
}
struct Meta {
    queue: RefCell<VecDeque<(String, Vec<u8>)>>,
    calls: RefCell<Vec<String>>,
}
impl version::Transport for Meta {
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
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unexpected metadata call");
        assert_eq!(u, url);
        Ok(b)
    }
}
struct Files {
    queue: Mutex<VecDeque<Vec<u8>>>,
    calls: Mutex<usize>,
    change: Option<PathBuf>,
}
impl Files {
    fn new(bytes: Vec<Vec<u8>>) -> Self {
        Self {
            queue: Mutex::new(bytes.into()),
            calls: Mutex::new(0),
            change: None,
        }
    }
}
impl install::Transport for Files {
    fn fetch(&self, url: &str, out: &mut dyn Write) -> Result<u16, install::Error> {
        assert_eq!(url, FILE);
        *self.calls.lock().unwrap() += 1;
        if let Some(p) = &self.change {
            fs::write(p, manifest(json!("after")))?;
        }
        out.write_all(
            &self
                .queue
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected file call"),
        )?;
        Ok(200)
    }
}
fn injected(
    args: &[String],
    svc: &version::Service<Meta>,
    files: &Files,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    match managed_request(args, err)? {
        Ok(ManagedRequest::Vanilla(p)) => install::vanilla::run_cli(p, svc, files, out, err),
        Ok(ManagedRequest::Legacy(_)) => panic!("loader execution forbidden in fixture"),
        Err(code) => Ok(code),
    }
}
fn check_file(o: &Owned, id: &[u8], links: u64) -> Snap {
    let mut p = b"versions/".to_vec();
    p.extend(id);
    p.push(b'/');
    p.extend(id);
    p.extend(b".jar");
    let dest = o.destination.join(store::path_from_bytes(&p).unwrap());
    let a = snap(&dest);
    let b = snap(&o.cas.join(store::object_path(&hash(DATA)).unwrap()));
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
    a
}
#[test]
fn managed_initial_manifest_failures_do_not_plan_or_open_cas() {
    let cases: Vec<(Option<Vec<u8>>, &str)> = vec![
        (None, "FileNotFound"),
        (
            Some(vec![b' '; instances::MAX_MANIFEST_BYTES + 1]),
            "InstanceManifestTooLarge",
        ),
        (Some(b"{".to_vec()), "UnexpectedEndOfInput"),
        (
            Some(br#"{"id":"demo","name":"n","version_id":"v","version_id":"v"}"#.to_vec()),
            "DuplicateField",
        ),
        (
            Some(br#"{"id":"demo","name":"n","version_id":"v","source":"bad"}"#.to_vec()),
            "InvalidEnumTag",
        ),
        (
            Some(br#"{"id":"other","name":"n","version_id":"v"}"#.to_vec()),
            "InstanceIdMismatch",
        ),
    ];
    for (bytes, name) in cases {
        let o = Owned::new(bytes.as_deref());
        let svc = o.service(json!("v"));
        let files = Files::new(vec![]);
        let mut out = vec![];
        let mut err = vec![];
        let mut args = o.args();
        args.extend(["--source".into(), "official".into()]);
        assert_eq!(
            injected(&args, &svc, &files, &mut out, &mut err).unwrap(),
            1
        );
        assert!(
            String::from_utf8(err).unwrap().contains(name),
            "expected {name}"
        );
        assert!(out.is_empty());
        assert!(svc.transport.calls.borrow().is_empty());
        assert_eq!(*files.calls.lock().unwrap(), 0);
        assert!(!o.cas.exists());
        assert!(!o.destination.exists());
        o.check();
    }
}
#[test]
fn managed_loader_exclusion_and_option_precedence_do_not_resolve() {
    let default_args = vec!["demo".to_owned()];
    let defaults = parse(&default_args).unwrap();
    assert_eq!(
        (
            defaults.root,
            defaults.store,
            defaults.workers,
            defaults.retries,
            defaults.policy.as_str()
        ),
        ("instances", ".jmclcore", 8, 2, "auto")
    );
    assert!(
        defaults.java.is_empty()
            && defaults.source.is_none()
            && defaults.os.is_none()
            && defaults.arch.is_none()
    );
    let o = Owned::new(Some(
        br#"{"id":"demo","name":"n","version_id":"v","fabric_loader":"f"}"#,
    ));
    let mut err = vec![];
    assert!(matches!(
        managed_request(&o.args(), &mut err).unwrap(),
        Ok(ManagedRequest::Legacy(_))
    ));
    assert!(err.is_empty());
    assert!(!o.cas.exists());
    let o = Owned::new(Some(&manifest(json!([255]))));
    let mut args = o.args();
    args.extend([
        "--source".into(),
        "bmclapi".into(),
        "--source".into(),
        "official".into(),
        "--arch".into(),
        "aarch64".into(),
        "--java".into(),
        "never-run".into(),
        "--java-policy".into(),
        "managed".into(),
    ]);
    let Ok(ManagedRequest::Vanilla(p)) = managed_request(&args, &mut err).unwrap() else {
        panic!()
    };
    assert_eq!(p.id, [255]);
    assert_eq!(p.source, Source::Official);
    assert_eq!(p.arch.as_deref(), Some("arm64"));
    for tail in [
        ["--java-policy", "bad"],
        ["--workers", "0"],
        ["--retries", "6"],
        ["--source", "bad"],
    ] {
        let mut args = o.args();
        args.extend(tail.map(str::to_owned));
        let result = request_with(
            &args,
            &mut vec![],
            |_, _| -> io::Result<()> { panic!("must reject before filesystem read") },
            |_, _| (),
        );
        assert!(matches!(result.unwrap(), Err(2)));
    }
    o.check();
}
struct StringPlanner;
impl Planner for StringPlanner {
    fn plan(&self, p: &Selection, t: Target) -> Result<plan::Plan, metadata::Error> {
        assert!(
            p.fabric_loader.is_none() && p.neoforge_version.is_none() && p.forge_version.is_none()
        );
        plan::build(
            &metadata::metadata(&body(json!(p.id))).unwrap(),
            None,
            p.source,
            t,
        )
    }
}
#[test]
fn managed_ordinary_cli_matches_unchanged_string_injected_path() {
    let mut outcomes = vec![];
    for byte in [false, true] {
        let o = Owned::new(Some(&manifest(json!("版本"))));
        let svc = o.service(json!("版本"));
        let files = Files::new(vec![DATA.to_vec()]);
        let mut out = vec![];
        let mut err = vec![];
        let code = if byte {
            injected(&o.args(), &svc, &files, &mut out, &mut err).unwrap()
        } else {
            run_with(&o.args(), &mut out, &mut err, &StringPlanner, &files).unwrap()
        };
        assert_eq!(code, 0);
        check_file(&o, "版本".as_bytes(), 2);
        outcomes.push((out, err, fs::read(o.receipt()).unwrap()));
    }
    assert_eq!(outcomes[0], outcomes[1]);
}
#[test]
fn managed_opaque_selection_read_once_and_captured_during_owned_change() {
    let o = Owned::new(Some(&manifest(json!([255]))));
    let svc = o.service(json!([255]));
    let mut files = Files::new(vec![DATA.to_vec()]);
    files.change = Some(o.manifest.clone());
    instances::reset_install_parse_count();
    let mut out = vec![];
    let mut err = vec![];
    assert_eq!(
        injected(&o.args(), &svc, &files, &mut out, &mut err).unwrap(),
        0
    );
    assert_eq!(instances::install_parse_count(), 1);
    assert_eq!(svc.transport.calls.borrow().len(), 2);
    assert_eq!(*files.calls.lock().unwrap(), 1);
    assert!(out.starts_with(b"version\t\xff\n"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(o.receipt()).unwrap()).unwrap()["version_id"],
        json!([255])
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&o.manifest).unwrap()).unwrap()["version_id"],
        "after"
    );
    check_file(&o, &[255], 2);
}
struct Fail;
impl Write for Fail {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("injected output failure"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn managed_failure_order_preserves_existing_receipts_and_peers() {
    for stage in ["file", "event", "receipt", "result"] {
        let o = Owned::new(Some(&manifest(json!("v"))));
        let svc = o.service(json!("v"));
        let old = if stage == "receipt" {
            fs::create_dir_all(o.receipt()).unwrap();
            None
        } else {
            Some(o.previous())
        };
        let files = Files::new(vec![if stage == "file" {
            b"bad".to_vec()
        } else {
            DATA.to_vec()
        }]);
        let mut out = vec![];
        let mut err = vec![];
        let result = match stage {
            "event" => injected(&o.args(), &svc, &files, &mut out, &mut Fail),
            "result" => injected(&o.args(), &svc, &files, &mut Fail, &mut err),
            _ => injected(&o.args(), &svc, &files, &mut out, &mut err),
        };
        if matches!(stage, "event" | "result") {
            assert!(result.is_err())
        } else {
            assert_eq!(result.unwrap(), 1);
            assert!(String::from_utf8_lossy(&err).contains(if stage == "file" {
                "download failed: DownloadSizeMismatch"
            } else {
                "failed to record completed installation: FileIoError"
            }));
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
        if stage != "file" {
            check_file(&o, b"v", 2);
        } else {
            assert!(
                !o.cas
                    .join(store::object_path(&hash(DATA)).unwrap())
                    .exists()
            );
            assert!(!o.destination.join("versions/v/v.jar").exists());
        }
        assert!(
            o.cas
                .join(format!("locks/sha1/{}.lock", hash(DATA)))
                .is_file()
        );
        o.check();
    }
}

#[test]
fn managed_root_errors_match_string_cli_names_and_order() {
    for stage in ["instances", "destination", "cas"] {
        let mut outcomes = vec![];
        for byte in [false, true] {
            let o = Owned::new(Some(&manifest(json!("v"))));
            let mut args = o.args();
            match stage {
                "instances" => {
                    let p = o.owner.path().join("blocked-instances");
                    fs::write(&p, b"blocked").unwrap();
                    args[2] = p.to_str().unwrap().into();
                }
                "destination" => {
                    fs::write(&o.destination, b"blocked").unwrap();
                    fs::write(&o.cas, b"blocked").unwrap();
                }
                _ => {
                    fs::write(&o.cas, b"blocked").unwrap();
                }
            }
            let svc = o.service(json!("v"));
            let files = Files::new(vec![]);
            let mut out = vec![];
            let mut err = vec![];
            let code = if byte {
                injected(&args, &svc, &files, &mut out, &mut err).unwrap()
            } else {
                run_with(&args, &mut out, &mut err, &StringPlanner, &files).unwrap()
            };
            assert_eq!(code, 1);
            assert!(out.is_empty());
            assert_eq!(*files.calls.lock().unwrap(), 0);
            if byte {
                assert_eq!(
                    svc.transport.calls.borrow().len(),
                    if stage == "instances" { 0 } else { 2 }
                );
            }
            assert!(!o.receipt().exists());
            o.check();
            outcomes.push(err);
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

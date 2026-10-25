use jmcl_core_rust::{
    install::{
        self,
        service::{Planner, Selection},
    },
    launch::instance_install,
    metadata::{self, Source},
    plan::{self, Plan, Target},
};
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Fixture;
impl Planner for Fixture {
    fn plan(&self, s: &Selection, t: Target) -> Result<Plan, metadata::Error> {
        let md=metadata::metadata(json!({"id":s.id,"type":"release","mainClass":"Fake.Main","libraries":[],"downloads":{"client":{"url":"https://example.invalid/benign","size":3,"sha1":format!("{:x}",Sha1::digest(b"abc"))}}}).to_string().as_bytes())?;
        plan::build(&md, None, s.source, t)
    }
}
struct Bytes {
    calls: AtomicUsize,
    bad: bool,
}
impl install::Transport for Bytes {
    fn fetch(&self, _: &str, sink: &mut dyn Write) -> Result<u16, install::Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        sink.write_all(if self.bad { b"bad" } else { b"abc" })
            .unwrap();
        Ok(200)
    }
}
#[test]
fn shared_immutable_artifact_receipts_and_failed_install() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("instances");
    let store = tmp.path().join("store");
    let source = Bytes {
        calls: AtomicUsize::new(0),
        bad: false,
    };
    for id in ["one", "two", "failed"] {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("instance.json"),
            json!({"id":id,"name":id,"version_id":"fixture","source":"official"}).to_string(),
        )
        .unwrap();
    }
    for id in ["one", "two"] {
        let args = [
            id,
            "--instances-directory",
            root.to_str().unwrap(),
            "--store-directory",
            store.to_str().unwrap(),
            "--source",
            "bmclapi",
        ]
        .map(Into::into);
        let (mut out, mut err) = (vec![], vec![]);
        assert_eq!(
            instance_install::run_with(&args, &mut out, &mut err, &Fixture, &source).unwrap(),
            0
        );
        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join(id).join(".minecraft/.jmcl/install.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["source"], Source::Bmclapi.name());
        assert!(String::from_utf8(out).unwrap().contains("hardLinked\t1"));
    }
    assert_eq!(source.calls.load(Ordering::Relaxed), 1);
    let one = root.join("one/.minecraft/versions/fixture/fixture.jar");
    let two = root.join("two/.minecraft/versions/fixture/fixture.jar");
    assert_eq!(fs::read(&one).unwrap(), b"abc");
    assert_eq!(fs::read(&two).unwrap(), b"abc");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&one).unwrap().ino(),
            fs::metadata(&two).unwrap().ino()
        );
        assert!(fs::metadata(&one).unwrap().nlink() >= 3);
    }
    let bad = Bytes {
        calls: AtomicUsize::new(0),
        bad: true,
    };
    let bad_store = tmp.path().join("bad-store");
    let args = [
        "failed",
        "--instances-directory",
        root.to_str().unwrap(),
        "--store-directory",
        bad_store.to_str().unwrap(),
        "--retries",
        "0",
    ]
    .map(Into::into);
    let (mut out, mut err) = (vec![], vec![]);
    assert_eq!(
        instance_install::run_with(&args, &mut out, &mut err, &Fixture, &bad).unwrap(),
        1
    );
    assert!(!root.join("failed/.minecraft/.jmcl/install.json").exists());
    assert!(
        !root
            .join("failed/.minecraft/versions/fixture/fixture.jar")
            .exists()
    );
}

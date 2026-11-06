//! Local synthetic handler-chain coverage, not a real game installation or GUI/Tauri acceptance.
//! No HTTP clients are constructed: both transport seams panic if touched. The only
//! artifact is inert text, not a JAR/ZIP. No loader, native extraction, assets or Java.
use super::*;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

struct Offline;
impl version::Transport for Offline {
    fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, crate::metadata::Error> {
        panic!("synthetic chain attempted metadata fetch: {url}")
    }
}
impl Transport for Offline {
    fn fetch(&self, url: &str, _: &mut dyn Write) -> Result<u16, Error> {
        panic!("synthetic chain attempted artifact fetch: {url}")
    }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha1::digest(b))
}
const BODY: &[u8] = b"inert synthetic artifact, not an archive";
fn seed(owner: &Path) -> (version::Service<Offline>, std::path::PathBuf) {
    let cache = owner.join("metadata-cache");
    fs::create_dir_all(&cache).unwrap();
    let url = "https://example.invalid/synthetic-version.json";
    let metadata = json!({"id":"synthetic","type":"release","mainClass":"NeverExecuted",
        "libraries":[], "downloads":{"client":{"url":"https://example.invalid/never-fetch",
        "size":BODY.len(), "sha1":hash(BODY)}}})
    .to_string();
    let manifest = json!({"latest":{"release":"synthetic","snapshot":"synthetic"},
        "versions":[{"id":"synthetic","type":"release","url":url,
        "sha1":hash(metadata.as_bytes()),"time":"now","releaseTime":"now"}]})
    .to_string();
    for (url, payload) in [(Source::Official.manifest_url(), manifest), (url, metadata)] {
        fs::write(
            cache.join(format!("{}.json", hash(url.as_bytes()))),
            json!({"v":1,"url":url,
            "fetched_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            "payload":payload})
            .to_string(),
        )
        .unwrap();
    }
    let digest = hash(BODY);
    let blob = owner
        .join("store/objects/sha1")
        .join(&digest[..2])
        .join(&digest);
    fs::create_dir_all(blob.parent().unwrap()).unwrap();
    fs::write(&blob, BODY).unwrap();
    let mut permissions = fs::metadata(&blob).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&blob, permissions).unwrap();
    (version::Service::new(Offline, cache), blob)
}
fn lines(out: &[u8], id: &str) -> Vec<Value> {
    let events: Vec<Value> = out
        .split(|b| *b == b'\n')
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_slice(s).unwrap())
        .collect();
    assert!(!events.is_empty());
    for e in &events {
        assert_eq!(e["protocol"], 1);
        assert_eq!(e["id"], id);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| e["event"] == "result" || e["event"] == "error")
            .count(),
        1
    );
    assert!(matches!(
        events.last().unwrap()["event"].as_str(),
        Some("result" | "error")
    ));
    events
}
fn instance(method: &str, params: Value) -> Value {
    let mut out = vec![];
    protocol::handle_line(
        json!({"protocol":1,"id":"instance-call","method":method,"params":params})
            .to_string()
            .as_bytes(),
        &mut out,
    )
    .unwrap();
    let result = lines(&out, "instance-call").pop().unwrap();
    assert_eq!(result["event"], "result", "{result}");
    result["result"].clone()
}
// Installation handlers use the existing test seam and the exact production
// vanilla parser/runner. This intentionally does NOT test top-level installation
// dispatch, native Session reopening, UI task state, or a packaged frontend.
fn install(
    svc: &version::Service<Offline>,
    owner: &Path,
    id: &str,
    prepare_only: bool,
) -> Vec<Value> {
    let raw = RawValue::from_string(
        json!({"id":"synthetic","directory":owner.join("instances").join(id).join(".minecraft"),
        "store_directory":owner.join("store"),"workers":1,"retries":0,"java_policy":"local"})
        .to_string(),
    )
    .unwrap();
    assert!(selected(Some(&raw)));
    let mut out = vec![];
    if prepare_only {
        prepare::run(&mut out, id, prepare::parse(Some(&raw)).unwrap(), svc).unwrap();
    } else {
        handle_with(&mut out, id, Some(&raw), svc, &Offline)
            .unwrap()
            .unwrap();
    }
    lines(&out, id)
}
fn assert_download(events: &[Value]) {
    assert!(
        !events
            .iter()
            .any(|e| e["event"] == "retry" || e["event"] == "error")
    );
    let files: Vec<_> = events.iter().filter(|e| e["event"] == "file").collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["file"]["status"], "cached");
    assert_eq!(files[0]["file"]["placement"], "hard_link");
    let progress = events
        .iter()
        .rev()
        .find(|e| e["event"] == "progress")
        .unwrap();
    assert_eq!(progress["progress"]["files_completed"], 1);
    assert_eq!(progress["progress"]["bytes_transferred"], 0);
    let r = &events.last().unwrap()["result"];
    assert_eq!(r["stage"], "download");
    assert_eq!(r["files_cached"], 1);
    assert_eq!(r["files_downloaded"], 0);
    assert_eq!(r["files_hard_linked"], 1);
    assert_eq!(r["bytes_transferred"], 0);
    assert_eq!(r["bytes_verified"], BODY.len());
}
#[test]
fn cached_two_instance_rpc_handler_chain_and_disk_reopen() {
    use std::os::unix::fs::MetadataExt;
    let owner = tempfile::tempdir().unwrap();
    let (svc, blob) = seed(owner.path());
    let directory = owner.path().join("instances");
    for id in ["one", "two"] {
        instance(
            "instance.create",
            json!({"directory":directory,"id":id,"name":id,"version_id":"synthetic","validate":false}),
        );
        assert_eq!(
            instance("instance.get", json!({"directory":directory,"id":id}))["installed"],
            false
        );
        assert_download(&install(&svc, owner.path(), id, false));
        let events = install(&svc, owner.path(), id, true);
        assert_eq!(events.len(), 1);
        let result = &events[0]["result"];
        assert_eq!(result["stage"], "prepare");
        for k in ["native_files", "projections_created", "projections_cached"] {
            assert_eq!(result[k], 0);
        }
        let game = directory.join(id).join(".minecraft");
        assert!(
            game.join(result["native_directory"].as_str().unwrap())
                .is_dir()
        );
        let artifact = game.join("versions/synthetic/synthetic.jar");
        assert_eq!(fs::read(&artifact).unwrap(), BODY);
        let a = fs::metadata(&artifact).unwrap();
        let b = fs::metadata(&blob).unwrap();
        assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
        assert!(a.permissions().readonly());
        assert_eq!(
            instance("instance.get", json!({"directory":directory,"id":id}))["installed"],
            true
        );
    }
    let receipt = |id: &str| directory.join(id).join(".minecraft/.jmcl/install.json");
    assert_ne!(
        fs::metadata(receipt("one")).unwrap().ino(),
        fs::metadata(receipt("two")).unwrap().ino()
    );
    assert!(fs::metadata(&blob).unwrap().nlink() >= 3);
    drop(svc);
    let reopened = version::Service::new(Offline, owner.path().join("metadata-cache"));
    for id in ["one", "two"] {
        assert_download(&install(&reopened, owner.path(), id, false));
        assert_eq!(
            instance("instance.get", json!({"directory":directory,"id":id}))["installed"],
            true
        );
    }
    let list = instance("instance.list", json!({"directory":directory}));
    assert_eq!(list["instances"].as_array().unwrap().len(), 2);
    assert!(
        list["instances"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["installed"] == true)
    );
    fs::remove_file(receipt("one")).unwrap();
    assert_eq!(
        instance("instance.get", json!({"directory":directory,"id":"one"}))["installed"],
        false
    );
    assert_eq!(
        instance("instance.get", json!({"directory":directory,"id":"two"}))["installed"],
        true
    );
    assert_eq!(fs::read(blob).unwrap(), BODY);
}
#[test]
fn prepare_failure_is_terminal_error_not_download_receipt_rollback() {
    let owner = tempfile::tempdir().unwrap();
    let (svc, _) = seed(owner.path());
    let directory = owner.path().join("instances");
    instance(
        "instance.create",
        json!({"directory":directory,"id":"blocked","name":"blocked","version_id":"synthetic","validate":false}),
    );
    assert_download(&install(&svc, owner.path(), "blocked", false));
    let game = directory.join("blocked/.minecraft");
    let receipt = fs::read(game.join(".jmcl/install.json")).unwrap();
    fs::write(game.join(".jmcl/runtime"), b"owned canary").unwrap();
    let events = install(&svc, owner.path(), "blocked", true);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"], "error");
    assert_eq!(events[0]["error"]["code"], "PREPARE_FAILED");
    assert_eq!(
        fs::read(game.join(".jmcl/runtime")).unwrap(),
        b"owned canary"
    );
    assert_eq!(fs::read(game.join(".jmcl/install.json")).unwrap(), receipt);
    assert_eq!(
        instance(
            "instance.get",
            json!({"directory":directory,"id":"blocked"})
        )["installed"],
        true
    );
}

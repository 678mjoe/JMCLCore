//! Full production RPC dispatcher + unchanged native Session, local synthetic bytes.
//! Run ONLY through run_instance_chain.py, which establishes network isolation
//! before executing this binary. No mock transport or dependency injection.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
use jmcl_lib::session::{Session, SessionError, SessionEvent};
use jmcl_lib::transport::LocalProcessTransport;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const OBJECT_HASH: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"; // SHA1(b"hello")
fn seed(root: &Path) {
    let metadata = json!({"id":"synthetic","type":"release","mainClass":"NeverExecuted","libraries":[],
        "downloads":{"client":{"url":"https://example.invalid/never-fetch-client","sha1":OBJECT_HASH,"size":5}}}).to_string();
    let manifest = json!({"latest":{"release":"synthetic","snapshot":"synthetic"},
        "versions":[{"id":"synthetic","type":"release","url":"https://example.invalid/synthetic-version.json",
        "time":"now","releaseTime":"now"}]}).to_string();
    let cache = root.join(".jmclcore/metadata-cache");
    fs::create_dir_all(&cache).unwrap();
    // Filenames are SHA1 of the exact production URL, not arbitrary cache keys.
    for (url, hash, payload) in [
        (
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            "8d52f11f50b848c491fb17b27bff394ff6ffbd16",
            manifest,
        ),
        (
            "https://example.invalid/synthetic-version.json",
            "b847965a629d771c40c5b9959347311e88d26707",
            metadata,
        ),
    ] {
        fs::write(
            cache.join(format!("{hash}.json")),
            json!({"v":1,"url":url,"payload":payload,
            "fetched_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()})
            .to_string(),
        )
        .unwrap();
    }
    let blob = root.join("store/objects/sha1/aa").join(OBJECT_HASH);
    fs::create_dir_all(blob.parent().unwrap()).unwrap();
    fs::write(&blob, b"hello").unwrap(); // Deliberately not a ZIP/JAR.
    let mut perms = fs::metadata(&blob).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(blob, perms).unwrap();
}
async fn open(root: &Path) -> Session {
    let binary = std::env::var("JMCL_BRIDGE_BINARY").unwrap();
    Session::establish(Box::new(
        LocalProcessTransport::spawn(Path::new(&binary), &["rpc"], Some(root))
            .await
            .unwrap(),
    ))
    .await
    .unwrap()
}
async fn call(
    session: &Session,
    method: &str,
    params: Value,
) -> Result<(Value, Vec<Value>), SessionError> {
    let mut events = vec![];
    let result = session
        .request(method, params, |event| match event {
            SessionEvent::Event(v) => events.push(v),
            SessionEvent::Diagnostic(s) => eprintln!("Core diagnostic: {s}"),
        })
        .await?;
    assert!(events
        .iter()
        .all(|v| v["event"] != "error" && v["event"] != "result"));
    assert!(events
        .iter()
        .all(|v| v["id"].as_str().is_some_and(|id| id.starts_with("req-"))));
    Ok((result, events))
}
fn params(root: &Path, id: &str) -> Value {
    json!({"id":"synthetic","directory":root.join("instances").join(id).join(".minecraft"),
        "store_directory":root.join("store"),"workers":1,"retries":0,"java_policy":"local"})
}
async fn status(session: &Session, root: &Path, id: &str) -> bool {
    call(
        session,
        "instance.get",
        json!({"directory":root.join("instances"),"id":id}),
    )
    .await
    .unwrap()
    .0["installed"]
        .as_bool()
        .unwrap()
}
async fn execute(session: &Session, root: &Path, id: &str) {
    let (result, events) = call(session, "install.execute", params(root, id))
        .await
        .unwrap();
    assert_eq!(result["stage"], "download");
    for key in ["files_total", "files_cached", "files_hard_linked"] {
        assert_eq!(result[key], 1);
    }
    for key in ["files_downloaded", "files_copied", "bytes_transferred"] {
        assert_eq!(result[key], 0);
    }
    assert_eq!(result["bytes_verified"], 5);
    assert!(!events.iter().any(|v| v["event"] == "retry"));
    let files: Vec<_> = events.iter().filter(|v| v["event"] == "file").collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["file"]["status"], "cached");
    assert_eq!(files[0]["file"]["placement"], "hard_link");
    let progress = events
        .iter()
        .rev()
        .find(|v| v["event"] == "progress")
        .unwrap();
    assert_eq!(progress["progress"]["files_completed"], 1);
    assert_eq!(progress["progress"]["bytes_transferred"], 0);
}
#[tokio::test]
async fn isolated_native_session_cached_instance_chain() {
    // The runner is mandatory. A filter must already be inherited at startup;
    // the runner also actively probes forbidden syscalls before starting us.
    assert_eq!(
        std::env::var("JMCL_NATIVE_CHAIN_GUARDED").as_deref(),
        Ok("1")
    );
    assert_eq!(unsafe { libc::prctl(libc::PR_GET_SECCOMP, 0, 0, 0, 0) }, 2);
    tokio::time::timeout(Duration::from_secs(30), async {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path();
        seed(root);
        let session = open(root).await;
        for id in ["one", "two", "blocked"] {
            call(
                &session,
                "instance.create",
                json!({"directory":root.join("instances"),"id":id,
                "name":id,"version_id":"synthetic","validate":false}),
            )
            .await
            .unwrap();
            assert!(!status(&session, root, id).await);
            execute(&session, root, id).await;
            if id == "blocked" {
                continue;
            }
            let (prepared, events) = call(&session, "install.prepare", params(root, id))
                .await
                .unwrap();
            assert!(events.is_empty());
            assert_eq!(prepared["stage"], "prepare");
            for key in ["native_files", "projections_created", "projections_cached"] {
                assert_eq!(prepared[key], 0);
            }
            let game = root.join("instances").join(id).join(".minecraft");
            assert!(game
                .join(prepared["native_directory"].as_str().unwrap())
                .is_dir());
            let artifact = game.join("versions/synthetic/synthetic.jar");
            assert_eq!(fs::read(&artifact).unwrap(), b"hello");
            let a = fs::metadata(&artifact).unwrap();
            let b = fs::metadata(root.join("store/objects/sha1/aa").join(OBJECT_HASH)).unwrap();
            assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
            assert!(a.permissions().readonly());
            assert!(status(&session, root, id).await);
        }
        let receipt = |id: &str| {
            root.join("instances")
                .join(id)
                .join(".minecraft/.jmcl/install.json")
        };
        assert_ne!(
            fs::metadata(receipt("one")).unwrap().ino(),
            fs::metadata(receipt("two")).unwrap().ino()
        );
        let before = fs::read(receipt("blocked")).unwrap();
        let canary = root.join("instances/blocked/.minecraft/.jmcl/runtime");
        fs::write(&canary, b"owned canary").unwrap();
        assert!(
            matches!(call(&session,"install.prepare",params(root,"blocked")).await,
            Err(SessionError::Rpc { code, .. }) if code == "PREPARE_FAILED")
        );
        assert_eq!(fs::read(&canary).unwrap(), b"owned canary");
        assert_eq!(fs::read(receipt("blocked")).unwrap(), before);
        assert!(status(&session, root, "blocked").await); // receipt records download completion only
        session.close().await;
        session.close().await;
        assert!(matches!(session.request("ping",json!({}),|_|{}).await,
            Err(SessionError::Transport { code, .. }) if code == "SESSION_CLOSED"));
        drop(session);
        let reopened = open(root).await; // New real Core child, not merely a recreated Service.
        for id in ["one", "two", "blocked"] {
            assert!(status(&reopened, root, id).await);
        }
        let list = call(
            &reopened,
            "instance.list",
            json!({"directory":root.join("instances")}),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(list["instances"].as_array().unwrap().len(), 3);
        assert!(list["instances"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["installed"] == true));
        execute(&reopened, root, "two").await;
        fs::remove_file(receipt("one")).unwrap();
        assert!(!status(&reopened, root, "one").await);
        assert!(status(&reopened, root, "two").await);
        assert_eq!(
            fs::read(root.join("store/objects/sha1/aa").join(OBJECT_HASH)).unwrap(),
            b"hello"
        );
        assert_eq!(
            call(&reopened, "ping", json!({})).await.unwrap().0["pong"],
            true
        );
        reopened.close().await;
    })
    .await
    .expect("bounded isolated native Session chain");
}

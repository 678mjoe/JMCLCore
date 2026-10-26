//! Real executable, unchanged native bridge, private local fixtures only.
use jmcl_lib::session::{Session, SessionError, SessionEvent};
use jmcl_lib::transport::LocalProcessTransport;
use serde_json::{json, Value};
use std::{fs, io::Write, path::Path, time::Duration};

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

#[tokio::test]
async fn cached_modpack_events_and_close_reopen_preserve_disk_state() {
    // A valid cached object means the fixture has no acquisition dependency.
    // If caching regresses the only URL is a local non-listening endpoint,
    // never a live provider. No runtime installer or game is involved.
    tokio::time::timeout(Duration::from_secs(20), async {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let hash = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"; // SHA-1("hello")
        let store = root.join("store");
        let object_dir = store.join("objects/sha1/aa");
        fs::create_dir_all(&object_dir).unwrap();
        fs::write(object_dir.join(hash), b"hello").unwrap();
        let pack = root.join("fixture.mrpack");
        let mut archive = zip::ZipWriter::new(fs::File::create(&pack).unwrap());
        archive
            .start_file(
                "modrinth.index.json",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        archive
            .write_all(
                &serde_json::to_vec(&json!({
                    "formatVersion":1,"game":"minecraft","name":"Bridge fixture","versionId":"v1",
                    "dependencies":{"minecraft":"1.21"},
                    "files":[{"path":"mods/a.jar","hashes":{"sha1":hash},"fileSize":5,
                        "downloads":["https://127.0.0.1:1/never-download"]}]
                }))
                .unwrap(),
            )
            .unwrap();
        archive.finish().unwrap();
        let instances = root.join("instances");
        let session = open(root).await;
        let mut events = vec![];
        let result = session
            .request(
                "modpack.install",
                json!({
                    "directory":instances,"store_directory":store,"id":"bridge","file":pack,
                    "workers":1,"retries":0
                }),
                |event| match event {
                    SessionEvent::Event(event) => events.push(event),
                    SessionEvent::Diagnostic(_) => panic!("unexpected diagnostic"),
                },
            )
            .await
            .unwrap();
        assert_eq!(result["files_installed"], 1);
        assert_eq!(result["overrides_written"], 0);
        assert!(
            !events.is_empty(),
            "real progress must reach the native callback"
        );
        let files: Vec<&Value> = events.iter().filter(|e| e["event"] == "file").collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["file"]["status"], "cached");
        assert!(!events.iter().any(|e| e["event"] == "retry"));
        let progress: Vec<&Value> = events.iter().filter(|e| e["event"] == "progress").collect();
        assert!(!progress.is_empty());
        assert_eq!(progress.last().unwrap()["progress"]["files_completed"], 1);
        assert_eq!(progress.last().unwrap()["progress"]["bytes_verified"], 5);
        assert_eq!(progress.last().unwrap()["progress"]["bytes_transferred"], 0);
        assert!(events
            .iter()
            .all(|e| e["id"].as_str().is_some_and(|id| id.starts_with("req-"))));
        assert!(events
            .iter()
            .all(|e| e["event"] != "result" && e["event"] != "error"));
        let installed = instances.join("bridge/.minecraft/mods/a.jar");
        assert_eq!(fs::read(&installed).unwrap(), b"hello");
        session.close().await;
        session.close().await;
        assert!(matches!(session.request("ping", json!({}), |_| {}).await,
            Err(SessionError::Transport { ref code, .. }) if code == "SESSION_CLOSED"));
        let reopened = open(root).await;
        let loaded = reopened
            .request(
                "instance.get",
                json!({"directory":instances,"id":"bridge"}),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(loaded["id"], "bridge");
        assert_eq!(
            loaded["installed"], false,
            "pack import is not game installation"
        );
        assert_eq!(fs::read(installed).unwrap(), b"hello");
        assert_eq!(fs::read(object_dir.join(hash)).unwrap(), b"hello");
        assert_eq!(
            reopened.request("ping", json!({}), |_| {}).await.unwrap()["pong"],
            true
        );
        reopened.close().await;
        println!("real native callback events: {}", events.len());
    })
    .await
    .expect("bounded local bridge lifecycle");
}

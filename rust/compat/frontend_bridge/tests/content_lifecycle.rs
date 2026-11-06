//! Local plain-file content lifecycle through unchanged native Session and real Core.
//! Use the guarded runner --scenario content; these .jar/.zip files are NOT archives.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
use jmcl_lib::session::{Session, SessionError, SessionEvent};
use jmcl_lib::transport::LocalProcessTransport;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
const FAMILIES: [(&str, &str); 2] = [("mods", "manual.jar"), ("resourcepacks", "manual.zip")];
fn game(root: &Path, id: &str) -> PathBuf {
    root.join("instances").join(id).join(".minecraft")
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
async fn call(session: &Session, method: &str, params: Value) -> Result<Value, SessionError> {
    let mut events = vec![];
    let result = session
        .request(method, params, |event| match event {
            SessionEvent::Event(e) => events.push(e),
            SessionEvent::Diagnostic(s) => eprintln!("Core diagnostic: {s}"),
        })
        .await;
    assert!(
        events.is_empty(),
        "Local content operations must not emit acquisition progress: {events:?}"
    );
    result
}
async fn listing(session: &Session, root: &Path, id: &str, family: &str) -> Value {
    call(
        session,
        &format!("{family}.list"),
        json!({"directory":game(root,id)}),
    )
    .await
    .unwrap()
}
fn selector(root: &Path, id: &str, family: &str, file: &str) -> Value {
    json!({"directory":game(root,id),"file":format!("{family}/{file}")})
}
fn assert_entry(v: &Value, family: &str, file: &str, enabled: bool) {
    assert_eq!(v["file"], format!("{family}/{file}"));
    assert_eq!(v["source"]["provider"], "manual");
    for key in ["project_id", "version_id", "slug"] {
        assert!(v["source"][key].is_null());
    }
    assert_eq!(v["sha1"], SHA1);
    assert_eq!(v["size"], 5);
    assert_eq!(v["enabled"], enabled);
    assert_eq!(v["as_dependency"], false);
}
#[tokio::test]
async fn isolated_native_session_content_lifecycle() {
    assert_eq!(
        std::env::var("JMCL_NATIVE_CHAIN_GUARDED").as_deref(),
        Ok("1")
    );
    assert_eq!(unsafe { libc::prctl(libc::PR_GET_SECCOMP, 0, 0, 0, 0) }, 2);
    tokio::time::timeout(Duration::from_secs(30), async {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path();
        let store = root.join("store");
        let blob = store.join("objects/sha1/aa").join(SHA1);
        let session = open(root).await;
        for id in ["one", "two"] {
            call(
                &session,
                "instance.create",
                json!({"directory":root.join("instances"),"id":id,
                "name":id,"version_id":"synthetic","validate":false}),
            )
            .await
            .unwrap();
            for (family, file) in FAMILIES {
                let path = game(root, id).join(family).join(file);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, b"hello").unwrap();
                let initial = listing(&session, root, id, family).await;
                assert_eq!(initial["entries"], json!([]));
                assert_eq!(initial["unmanaged"], json!([format!("{family}/{file}")]));
                let adopted = call(
                    &session,
                    &format!("{family}.adopt"),
                    json!({"directory":game(root,id),"store_directory":store}),
                )
                .await
                .unwrap();
                assert_eq!(adopted["adopted"].as_array().unwrap().len(), 1);
                assert_eq!(adopted["adopted"][0]["file"], format!("{family}/{file}"));
                let listed = listing(&session, root, id, family).await;
                assert_eq!(listed["unmanaged"], json!([]));
                assert_eq!(listed["entries"].as_array().unwrap().len(), 1);
                assert_entry(&listed["entries"][0], family, file, true);
                let registry = game(root, id).join(".jmcl/content.json");
                let before = fs::read(&registry).unwrap();
                let again = call(
                    &session,
                    &format!("{family}.adopt"),
                    json!({"directory":game(root,id),"store_directory":store}),
                )
                .await
                .unwrap();
                assert_eq!(again["adopted"], json!([]));
                assert_eq!(fs::read(&registry).unwrap(), before);
                let a = fs::metadata(&path).unwrap();
                let b = fs::metadata(&blob).unwrap();
                assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
                assert!(a.permissions().readonly());
                // set_version excludes Manual before the first provider get.
                let mut change = json!({"directory":game(root,id),"store_directory":store,
                    "minecraft_version":"1.21.4","project":"manual","version_id":"synthetic"});
                if family == "mods" {
                    change["loader"] = json!("fabric");
                }
                assert!(
                    matches!(call(&session,&format!("{family}.set-version"),change).await,
                    Err(SessionError::Rpc{code,..}) if code=="CONTENT_NOT_FOUND")
                );
                assert_eq!(fs::read(&registry).unwrap(), before);
                assert_eq!(fs::read(path).unwrap(), b"hello");
            }
        }
        assert!(fs::metadata(&blob).unwrap().nlink() >= 5);
        let peer_registry = fs::read(game(root, "two").join(".jmcl/content.json")).unwrap();
        for (family, file) in FAMILIES {
            let params = selector(root, "one", family, file);
            let already = call(&session, &format!("{family}.enable"), params.clone())
                .await
                .unwrap();
            assert_eq!(already["changed"], false);
            for expected in [true, false] {
                let result = call(&session, &format!("{family}.disable"), params.clone())
                    .await
                    .unwrap();
                assert_eq!(result["changed"], expected);
                assert_entry(&result["entry"], family, file, false);
            }
            assert!(!game(root, "one").join(family).join(file).exists());
            assert_eq!(
                fs::read(
                    game(root, "one")
                        .join(family)
                        .join(format!("{file}.disabled"))
                )
                .unwrap(),
                b"hello"
            );
            assert_eq!(
                fs::read(game(root, "two").join(family).join(file)).unwrap(),
                b"hello"
            );
        }
        session.close().await;
        drop(session);
        let reopened = open(root).await;
        for (family, file) in FAMILIES {
            let listed = listing(&reopened, root, "one", family).await;
            assert_eq!(listed["unmanaged"], json!([]));
            assert_entry(&listed["entries"][0], family, file, false);
            let params = selector(root, "one", family, file);
            for expected in [true, false] {
                let result = call(&reopened, &format!("{family}.enable"), params.clone())
                    .await
                    .unwrap();
                assert_eq!(result["changed"], expected);
                assert_entry(&result["entry"], family, file, true);
            }
            assert!(!game(root, "one")
                .join(family)
                .join(format!("{file}.disabled"))
                .exists());
            assert_eq!(
                fs::read(game(root, "one").join(family).join(file)).unwrap(),
                b"hello"
            );
            // Remove one disabled entry and one enabled entry, exercising both paths.
            if family == "mods" {
                call(&reopened, "mods.disable", params.clone())
                    .await
                    .unwrap();
            }
            let removed = call(&reopened, &format!("{family}.remove"), params.clone())
                .await
                .unwrap();
            assert_entry(&removed["removed"], family, file, family != "mods");
            assert!(!game(root, "one").join(family).join(file).exists());
            assert!(!game(root, "one")
                .join(family)
                .join(format!("{file}.disabled"))
                .exists());
            let empty = listing(&reopened, root, "one", family).await;
            assert_eq!(empty["entries"], json!([]));
            assert_eq!(empty["unmanaged"], json!([]));
            assert!(
                matches!(call(&reopened,&format!("{family}.remove"),params).await,
                Err(SessionError::Rpc{code,..}) if code=="CONTENT_NOT_FOUND")
            );
            let peer = listing(&reopened, root, "two", family).await;
            assert_entry(&peer["entries"][0], family, file, true);
            assert_eq!(
                fs::read(game(root, "two").join(family).join(file)).unwrap(),
                b"hello"
            );
        }
        assert_eq!(
            fs::read(game(root, "two").join(".jmcl/content.json")).unwrap(),
            peer_registry
        );
        assert_eq!(fs::read(&blob).unwrap(), b"hello");
        assert_eq!(fs::metadata(&blob).unwrap().nlink(), 3);
        reopened.close().await;
        drop(reopened);
        let final_session = open(root).await;
        for (family, file) in FAMILIES {
            assert_eq!(
                listing(&final_session, root, "one", family).await["entries"],
                json!([])
            );
            assert_entry(
                &listing(&final_session, root, "two", family).await["entries"][0],
                family,
                file,
                true,
            );
        }
        final_session.close().await;
    })
    .await
    .expect("bounded isolated content lifecycle");
}

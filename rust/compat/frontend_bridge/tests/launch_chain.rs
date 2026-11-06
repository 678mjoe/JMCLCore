//! Real unchanged native Session -> production Core launch.execute -> benign
//! native child. Only the explicit runner may execute this opt-in test.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
use base64::{engine::general_purpose::STANDARD, Engine};
use jmcl_lib::{
    session::{Session, SessionError, SessionEvent},
    transport::LocalProcessTransport,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ACCESS: &str = "synthetic-access-secret";
const SESSION: &str = "synthetic-session-secret";
const OBJECT_HASH: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

fn seed(root: &Path) {
    let metadata = json!({"id":"synthetic","type":"release","mainClass":"NativeLaunchFixture",
        "libraries":[],"javaVersion":{"component":"fixture","majorVersion":17},
        "downloads":{"client":{"url":"https://example.invalid/never-fetch-client","sha1":OBJECT_HASH,"size":5}},
        "arguments":{"jvm":["-Djava.library.path=${natives_directory}"],
        "game":["${auth_access_token}","${auth_session}","${auth_player_name}","${auth_uuid}","literal argument",";not shell"]}}).to_string();
    let manifest = json!({"latest":{"release":"synthetic","snapshot":"synthetic"},
        "versions":[{"id":"synthetic","type":"release","url":"https://example.invalid/synthetic-version.json",
        "time":"now","releaseTime":"now"}]}).to_string();
    let cache = root.join(".jmclcore/metadata-cache");
    fs::create_dir_all(&cache).unwrap();
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
        fs::write(cache.join(format!("{hash}.json")), json!({"v":1,"url":url,
            "payload":payload,"fetched_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()}).to_string()).unwrap();
    }
    let artifact = game(root).join("versions/synthetic/synthetic.jar");
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::write(artifact, b"hello").unwrap(); // Ordinary bytes; never opened as an archive.
    fs::copy(
        std::env::var("JMCL_NATIVE_LAUNCH_CHILD").unwrap(),
        root.join("native-child"),
    )
    .unwrap();
}

fn trace(record: Value) {
    let path = std::env::var("JMCL_NATIVE_LAUNCH_TRACE").unwrap();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    serde_json::to_writer(&mut file, &record).unwrap();
    file.write_all(b"\n").unwrap();
    file.flush().unwrap();
}

async fn open(root: &Path, number: u64) -> Session {
    let binary = std::env::var("JMCL_BRIDGE_BINARY").unwrap();
    let session = Session::establish(Box::new(
        LocalProcessTransport::spawn(Path::new(&binary), &["rpc"], Some(root))
            .await
            .unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(session.identity().protocol, 1);
    trace(json!({"type":"session_open","session":number,"identity":session.identity()}));
    session
}

async fn call(
    session: &Session,
    number: u64,
    method: &str,
    params: Value,
) -> (Result<Value, SessionError>, Vec<Value>) {
    let mut events = vec![];
    let mut raw = vec![];
    let result = session
        .request(method, params.clone(), |event| {
            raw.push(serde_json::to_value(&event).unwrap());
            match event {
                SessionEvent::Event(v) => events.push(v),
                SessionEvent::Diagnostic(text) => panic!("Unexpected diagnostic: {text}"),
            }
        })
        .await;
    let outcome = match &result {
        Ok(value) => json!({"kind":"result","data":value}),
        Err(error) => json!({"kind":"error","data":error}),
    };
    trace(json!({"type":"request","session":number,"method":method,
        "params":params,"events":raw,"outcome":outcome}));
    let visible = serde_json::to_string(&json!({"events":events,"outcome":outcome})).unwrap();
    for secret in [ACCESS, SESSION] {
        assert!(!visible.contains(secret));
    }
    assert!(events
        .iter()
        .all(|event| event["event"] != "result" && event["event"] != "error"));
    (result, events)
}

fn game(root: &Path) -> std::path::PathBuf {
    root.join("instances/demo/.minecraft")
}

fn params(root: &Path, exit: u8) -> Value {
    // Match TasksProvider -> CoreSession.launchExecute's real wire shape:
    // instance-local game directory and no java_paths with an explicit override.
    json!({"id":"synthetic","directory":game(root),"source":"official",
        "auth":{"player_name":"Fixture 玩家","uuid":"synthetic-uuid","access_token":ACCESS,"session":SESSION},
        "java_override":root.join("native-child"),"java_policy":"local",
        "store_directory":root.join("never-create-store"),
        "options":{"include_default_user_jvm":false,"extra_game_args":[exit.to_string()]}})
}

fn cleaned(root: &Path) {
    let natives = game(root).join(".jmcl/runtime/natives");
    assert!(natives.is_dir());
    assert_eq!(fs::read_dir(natives).unwrap().count(), 0);
    assert!(!root.join("never-create-store").exists());
    assert_eq!(
        fs::read(game(root).join("versions/synthetic/synthetic.jar")).unwrap(),
        b"hello"
    );
}

fn verify_launch(root: &Path, result: &Value, events: &[Value], exit: u8) -> u64 {
    assert_eq!(result["stage"], "launch");
    assert_eq!(result["version_id"], "synthetic");
    assert_eq!(result["native_cleanup"], "removed");
    let process = &result["process"];
    assert_eq!(process["termination"], "exited");
    assert_eq!(process["exit_code"], exit);
    assert!(process["signal"].is_null() && process["status"].is_null());
    assert!(process["duration_ms"].as_u64().is_some());
    assert!(process["max_rss"].as_u64().is_some());
    let pid = process["pid"].as_u64().unwrap();
    assert!(pid > 0);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert_eq!(events[0]["event"], "started");
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "started")
            .count(),
        1
    );
    assert_eq!(events[0]["started"]["pid"], pid);
    assert_eq!(events[0]["started"]["stage"], "launch");
    assert_eq!(events[0]["started"]["version_id"], "synthetic");
    let argv = events[0]["started"]["redacted_argv"].as_array().unwrap();
    assert_eq!(argv[0], root.join("native-child").to_str().unwrap());
    assert_eq!(argv.iter().filter(|arg| **arg == "<redacted>").count(), 2);
    assert!(argv.contains(&json!("literal argument")) && argv.contains(&json!(";not shell")));
    let native = argv
        .iter()
        .filter_map(Value::as_str)
        .find_map(|arg| arg.strip_prefix("-Djava.library.path="))
        .unwrap();
    assert!(!Path::new(native).exists()); // Child wrote its canary here before exiting.
    let request = events[0]["id"].as_str().unwrap();
    assert!(request.starts_with("req-"));
    let mut stdout = vec![];
    let mut stderr = vec![];
    let mut stdout_parts = vec![];
    let mut stderr_parts = vec![];
    let mut sequence = 0;
    for event in events {
        assert_eq!(event["protocol"], 1);
        assert_eq!(event["source"], "official");
        assert_eq!(event["id"], request);
        let stream = event["event"].as_str().unwrap();
        if stream == "started" {
            continue;
        }
        assert!(matches!(stream, "stdout" | "stderr"));
        let log = &event[stream];
        sequence += 1;
        assert_eq!(log["sequence"], sequence);
        assert_eq!(log["encoding"], "base64");
        let bytes = STANDARD.decode(log["data"].as_str().unwrap()).unwrap();
        assert!(bytes.len() <= 16384);
        for secret in [ACCESS, SESSION] {
            assert!(!bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes()));
        }
        let end = log["end"].as_str().unwrap();
        assert!(matches!(end, "more" | "newline" | "eof"));
        let (all, parts) = if stream == "stdout" {
            (&mut stdout, &mut stdout_parts)
        } else {
            (&mut stderr, &mut stderr_parts)
        };
        all.extend_from_slice(&bytes);
        if end == "newline" {
            all.push(b'\n');
        }
        parts.push((bytes, end));
    }
    let mut expected = b"native child ready\n".to_vec();
    expected.extend(vec![b'x'; 16383]);
    expected.extend("中".as_bytes());
    expected.extend(vec![b'y'; 20000]);
    expected.extend(b"\n\ntoken=<redacted>; session=<redacted>\n");
    assert_eq!(stdout, expected);
    assert_eq!(stderr, b"stderr=<redacted>\nbinary\0\xff");
    assert!(stdout_parts
        .iter()
        .any(|(bytes, end)| bytes.is_empty() && *end == "newline"));
    assert_eq!(
        stdout_parts
            .iter()
            .filter(|(_, end)| *end == "more")
            .count(),
        2
    );
    assert_eq!(stdout_parts[1].0.len(), 16384);
    assert_eq!(stdout_parts[1].0.last(), Some(&0xe4));
    assert!(std::str::from_utf8(&stdout_parts[1].0).is_err());
    assert!(stdout_parts[2].0.starts_with(&[0xb8, 0xad]));
    assert!(std::str::from_utf8(&stdout).is_ok());
    assert_eq!(stderr_parts.last().unwrap().1, "eof");
    cleaned(root);
    pid
}

#[tokio::test]
async fn isolated_native_session_launch_execute_chain() {
    assert_eq!(
        std::env::var("JMCL_NATIVE_CHAIN_GUARDED").as_deref(),
        Ok("1")
    );
    assert_eq!(unsafe { libc::prctl(libc::PR_GET_SECCOMP, 0, 0, 0, 0) }, 2);
    tokio::time::timeout(Duration::from_secs(30), async {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path();
        seed(root);
        trace(json!({"type":"fixture","root":root}));
        let session = open(root, 1).await;
        // Failure occurs after native preparation, before child start. The
        // explicit missing override must fail, not search/download a runtime.
        let mut missing = params(root, 0);
        missing["java_override"] = json!(root.join("missing-native-child"));
        let (failed, events) = call(&session, 1, "launch.execute", missing).await;
        assert!(
            matches!(&failed, Err(SessionError::Rpc { code, .. }) if code == "JAVA_NOT_FOUND"),
            "{failed:?}"
        );
        assert!(events.is_empty());
        cleaned(root);
        let (result, events) = call(&session, 1, "launch.execute", params(root, 7)).await;
        let first_pid = verify_launch(root, &result.unwrap(), &events, 7);
        let (ping, events) = call(&session, 1, "ping", json!({})).await;
        assert!(ping.is_ok() && events.is_empty());
        // A nonzero game exit is still one successful RPC result. The same
        // Session remains usable and creates a fresh owned child on retry.
        let (result, events) = call(&session, 1, "launch.execute", params(root, 0)).await;
        assert_ne!(verify_launch(root, &result.unwrap(), &events, 0), first_pid);
        session.close().await;
        session.close().await;
        trace(json!({"type":"session_close","session":1}));
        let (closed, events) = call(&session, 1, "ping", json!({})).await;
        assert!(
            matches!(closed, Err(SessionError::Transport { code, .. }) if code == "SESSION_CLOSED")
        );
        assert!(events.is_empty());
        drop(session);
        let reopened = open(root, 2).await;
        let (result, events) = call(&reopened, 2, "launch.execute", params(root, 0)).await;
        verify_launch(root, &result.unwrap(), &events, 0);
        let (ping, events) = call(&reopened, 2, "ping", json!({})).await;
        assert!(ping.is_ok() && events.is_empty());
        reopened.close().await;
        trace(json!({"type":"session_close","session":2}));
        cleaned(root);
    })
    .await
    .expect("native launch chain exceeded bounded test deadline");
}

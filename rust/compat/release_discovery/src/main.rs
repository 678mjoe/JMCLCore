#[allow(dead_code)]
mod session;
#[allow(dead_code)]
mod transport;

use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    assert!(
        !cfg!(debug_assertions),
        "probe must be built in release mode"
    );
    assert!(std::env::var_os("JMCL_CORE").is_none());
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 3);
    assert_eq!(args[2], std::env::consts::ARCH, "native host architecture");
    let exe = std::env::current_exe().unwrap();
    let adjacent = exe.parent().unwrap().join(if cfg!(windows) {
        "jmcl-core.exe"
    } else {
        "jmcl-core"
    });
    assert_ne!(std::env::current_dir().unwrap(), exe.parent().unwrap());
    // The only production resolver invocation: no explicit path, no override.
    let result = transport::resolve_core_binary(None);
    if args[1] == "absent" {
        let error = result.expect_err("removed sidecar must not resolve elsewhere");
        assert_eq!(error, format!("{} not found near the application or in backend-binaries; set JMCL_CORE or pass binary_path", adjacent.file_name().unwrap().to_str().unwrap()));
        println!(
            "{}",
            json!({"scenario":"absent", "error":error,"arch":std::env::consts::ARCH,"debug_assertions":cfg!(debug_assertions)})
        );
        return;
    }
    assert_eq!(args[1], "present");
    let resolved: PathBuf = result.unwrap();
    assert_eq!(resolved, adjacent, "must use exact adjacent sidecar");
    let transport = transport::LocalProcessTransport::spawn(
        &resolved,
        &["rpc"],
        Some(&std::env::current_dir().unwrap()),
    )
    .await
    .unwrap();
    let session =
        session::Session::establish_with_timeout(Box::new(transport), Duration::from_secs(10))
            .await
            .unwrap();
    assert_eq!(session.identity().protocol, 1);
    let ping = tokio::time::timeout(
        Duration::from_secs(10),
        session.request("ping", json!({}), |_| {}),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(ping, json!({"pong": true}));
    session.close().await;
    let closed = session
        .request("ping", json!({}), |_| {})
        .await
        .unwrap_err();
    assert!(
        matches!(closed, session::SessionError::Transport {code, ..} if code == "SESSION_CLOSED")
    );
    println!(
        "{}",
        json!({"scenario":"present", "resolved":resolved,"identity":session.identity(),"ping":ping,"closed":true,"arch":std::env::consts::ARCH,"debug_assertions":cfg!(debug_assertions)})
    );
}

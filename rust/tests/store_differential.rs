//! Required cross-language GC and digest-lock checks, including Windows handles.
use jmcl_core_rust::store::{GcOptions, Store, object_path};
use sha1::{Digest, Sha1};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn fixture(root: &Path) -> (Store, String) {
    let store = Store::open(root).unwrap();
    for (bytes, linked) in [
        (b"eligible".as_slice(), false),
        (b"linked".as_slice(), true),
    ] {
        let hash = format!("{:x}", Sha1::digest(bytes));
        let object = root.join(object_path(&hash).unwrap());
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(object, bytes).unwrap();
        if linked {
            std::fs::hard_link(
                root.join(object_path(&hash).unwrap()),
                root.join("instance-link"),
            )
            .unwrap();
        }
    }
    std::fs::write(root.join("objects/sha1/junk"), b"ignored").unwrap();
    (store, format!("{:x}", Sha1::digest(b"eligible")))
}

#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE; mandatory in rewrite CI"]
fn native_gc_and_digest_lock_match_zig() {
    let oracle = std::env::var_os("JMCL_ZIG_ORACLE").expect("set pinned JMCL_ZIG_ORACLE");
    let temp = tempfile::tempdir().unwrap();
    let zig_root = temp.path().join("zig");
    let rust_root = temp.path().join("rust");
    fixture(&zig_root);
    let (rust, _) = fixture(&rust_root);
    for dry_run in [true, false] {
        let mut command = Command::new(&oracle);
        command
            .args(["store", "gc", "--store-directory"])
            .arg(&zig_root)
            .args(["--min-age-seconds", "0", "--json"]);
        if !dry_run {
            command.arg("--execute");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual = rust
            .collect(GcOptions {
                dry_run,
                min_age_seconds: 0,
            })
            .unwrap();
        assert_eq!(
            expected["result"]["summary"],
            serde_json::to_value(actual).unwrap()
        );
    }
    let shared = temp.path().join("shared");
    let (store, hash) = fixture(&shared);
    let guard = store.lock_object(&hash).unwrap();
    let mut child = Command::new(&oracle)
        .args(["store", "gc", "--store-directory"])
        .arg(&shared)
        .args(["--min-age-seconds", "0", "--execute", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    if child.try_wait().unwrap().is_some() {
        let output = child.wait_with_output().unwrap();
        let retained = shared.join(object_path(&hash).unwrap()).exists();
        drop(guard);
        let fresh = Command::new(&oracle)
            .args(["store", "gc", "--store-directory"])
            .arg(&shared)
            .args(["--min-age-seconds", "0", "--execute", "--json"])
            .output()
            .unwrap();
        panic!(
            "Zig exited while Rust held digest lock: status={:?}; stdout={}; stderr={}; retained={}; fresh_status={:?}; fresh_stdout={}; fresh_stderr={}; after_release_exists={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            retained,
            fresh.status,
            String::from_utf8_lossy(&fresh.stdout),
            String::from_utf8_lossy(&fresh.stderr),
            shared.join(object_path(&hash).unwrap()).exists()
        );
    }
    assert!(shared.join(object_path(&hash).unwrap()).exists());
    drop(guard);
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Zig did not complete after digest lock release");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["result"]["summary"]["objects_deleted"], 1);
    assert!(!shared.join(object_path(&hash).unwrap()).exists());
}

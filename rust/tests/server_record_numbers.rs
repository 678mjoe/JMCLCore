//! Persisted server records use Zig typed binary128 coercion, not RPC Value/f64.
//! Only synthetic files and one owned stdin-blocked process; never stop/signal,
//! launch Java, select a runtime, execute a game or contact a server.
#![cfg(target_os = "linux")]
use jmcl_core_rust::servers::{lifecycle::State, store::Installation};
fn state(token: &str) -> String {
    format!(
        r#"{{"pid":123,"supervisor_pid":456,"identity":"0123456789abcdef0123456789abcdef","java_path":"synthetic-only","started_at_ms":{token}}}"#
    )
}
fn marker(field: &str, token: &str) -> String {
    format!(
        r#"{{"schema_version":3,"version_id":"fixture","server_sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","server_size":{},"java_major_version":{},"launch_mode":"jar","launch_file":"server.jar","launch_sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","launch_size":{}}}"#,
        if field == "server_size" { token } else { "3" },
        if field == "java_major_version" {
            token
        } else {
            "21"
        },
        if field == "launch_size" { token } else { "3" }
    )
}
#[test]
fn persisted_precision_does_not_round_through_binary64() {
    for token in [
        "1.0000000000000001",
        r#""1.0000000000000001""#,
        r#""9223372036854775807.1""#,
    ] {
        assert!(
            serde_json::from_str::<State>(&state(token)).is_err(),
            "timestamp {token}"
        )
    }
    for field in ["server_size", "launch_size", "java_major_version"] {
        for token in ["3.0000000000000001", r#""3.0000000000000001""#] {
            assert!(
                serde_json::from_str::<Installation>(&marker(field, token)).is_err(),
                "{field}={token}"
            )
        }
    }
}
#[test]
fn persisted_signed_unsigned_boundaries_and_legacy_grammar() {
    for (token, n) in [
        ("-9223372036854775808", i64::MIN),
        (r#""-9223372036854775808.0""#, i64::MIN),
        ("9223372036854775807", i64::MAX),
        (r#""9223372036854775807.0""#, i64::MAX),
        ("9007199254740993.0", 9007199254740993),
        (r#""-9007199254740993e0""#, -9007199254740993),
        (r#""-0""#, 0),
        ("-0.0", 0),
        (r#""+1_0""#, 10),
        (r#""-1__0""#, -10),
        (r#""0x1.8p+1""#, 3),
        (r#""-0x1.8p+1""#, -3),
        (r#""1e1""#, 10),
        (r#""3.00000000000000000000000000000000001""#, 3),
    ] {
        assert_eq!(
            serde_json::from_str::<State>(&state(token))
                .unwrap()
                .started_at_ms,
            n,
            "{token}"
        )
    }
    for token in [
        r#""9223372036854775808""#,
        r#""-9223372036854775809""#,
        r#""-+1""#,
        r#""+-1""#,
        r#""--1""#,
        r#""++1""#,
        r#"" 1""#,
        r#""1_""#,
        r#""0x1p0""#,
        r#""NaN""#,
        "null",
        "true",
        "[]",
    ] {
        assert!(
            serde_json::from_str::<State>(&state(token)).is_err(),
            "{token}"
        )
    }
    for field in ["server_size", "launch_size"] {
        for (token, n) in [
            ("18446744073709551615", u64::MAX),
            (r#""18446744073709551615.0""#, u64::MAX),
            (r#""0x1.8p+1""#, 3),
            (r#""+1__0""#, 10),
            ("-0.0", 0),
        ] {
            let m = serde_json::from_str::<Installation>(&marker(field, token)).unwrap();
            assert_eq!(
                if field == "server_size" {
                    m.server_size
                } else {
                    m.launch_size
                },
                n
            );
        }
        for token in [
            "18446744073709551616",
            r#""18446744073709551615.1""#,
            r#""-1""#,
        ] {
            assert!(
                serde_json::from_str::<Installation>(&marker(field, token)).is_err(),
                "{field}={token}"
            )
        }
    }
    for (token, n) in [
        ("65535", 65535),
        (r#""65535e0""#, 65535),
        (r#""0x1.5p4""#, 21),
    ] {
        assert_eq!(
            serde_json::from_str::<Installation>(&marker("java_major_version", token))
                .unwrap()
                .java_major_version,
            n
        )
    }
    for token in ["65536", r#""65535.0000000000000001""#] {
        assert!(serde_json::from_str::<Installation>(&marker("java_major_version", token)).is_err())
    }
}

use jmcl_core_rust::servers;
use serde_json::{Value, json, value::RawValue};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
};
struct OwnedIdentity(Child);
impl OwnedIdentity {
    fn new() -> Self {
        let child = Command::new("python3")
            .args([
                "-u",
                "-c",
                "import sys; sys.stdin.buffer.read()",
                "-Djmcl.server.identity=0123456789abcdef0123456789abcdef",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // Spawn returns after exec with the identity already in argv. No
        // application readiness or signal handling is needed for status reads.
        Self(child)
    }
    fn alive(&mut self) {
        assert!(
            self.0.try_wait().unwrap().is_none(),
            "read-only request affected owned child"
        )
    }
}
impl Drop for OwnedIdentity {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let _ = self.0.wait();
    }
}
fn file_identity(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path).unwrap();
    (m.dev(), m.ino())
}
fn request(binary: Option<&str>, root: &Path, method: &str) -> Value {
    let params = json!({"directory":root,"id":"demo"});
    request_params(binary, method, &params.to_string())
}
fn request_params(binary: Option<&str>, method: &str, params: &str) -> Value {
    let raw = RawValue::from_string(params.to_owned()).unwrap();
    if let Some(binary) = binary {
        let mut child = Command::new(binary)
            .arg("rpc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            json!({"protocol":1,"id":"n","method":method,"params":raw})
        )
        .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        serde_json::from_slice(&out.stdout).unwrap()
    } else {
        match servers::handle(method, Some(&raw)).unwrap() {
            Ok(v) => json!({"protocol":1,"id":"n","event":"result","result":v}),
            Err(e) => {
                json!({"protocol":1,"id":"n","event":"error","error":{"code":e.code,"message":e.message}})
            }
        }
    }
}
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("demo/.jmcl")).unwrap();
    fs::write(
        root.join("demo/server.json"),
        br#"{"id":"demo","name":"Demo","version_id":"fixture"}"#,
    )
    .unwrap();
    fs::write(root.join("shared.jar"), b"abc").unwrap();
    fs::hard_link(root.join("shared.jar"), root.join("demo/server.jar")).unwrap();
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_ORACLE and JMCL_ZIG_SOURCE; status/get/logs only, no signals"]
fn persisted_server_records_match_pinned_source_without_process_control() {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").expect("pinned oracle required");
    let source = std::env::var("JMCL_ZIG_SOURCE").expect("pinned source required");
    let head = Command::new("git")
        .args(["-C", &source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        jmcl_core_rust::ORACLE_COMMIT
    );
    let mut child = OwnedIdentity::new();
    let mut cases = 0;
    // These are semantic state expectations, independent of either parser:
    // timestamps must parse exactly and be strictly positive to retain state.
    let signed = [
        ("1", Some(1_i64)),
        (r#""1e0""#, Some(1)),
        (r#""+1__0""#, Some(10)),
        (r#""0x1.8p+1""#, Some(3)),
        ("9007199254740993.0", Some(9007199254740993)),
        (r#""9223372036854775807.0""#, Some(i64::MAX)),
        (r#""-9223372036854775808.0""#, None),
        (r#""-9223372036854775809""#, None),
        (r#""9223372036854775808""#, None),
        ("1.0000000000000001", None),
        (r#""1.0000000000000001""#, None),
        (r#""3.00000000000000000000000000000000001""#, Some(3)),
        (r#""-0""#, None),
        (r#""-+1""#, None),
        (r#""+-1""#, None),
        (r#""--1""#, None),
        (r#""++1""#, None),
        (r#""0x1p0""#, None),
        (r#"" 1""#, None),
        ("null", None),
        ("true", None),
        ("[]", None),
    ];
    for (token, expected_started_at) in signed {
        let mut results = vec![];
        for binary in [Some(oracle.as_str()), None] {
            let dir = tempfile::tempdir().unwrap();
            fixture(dir.path());
            let path = dir.path().join("demo/.jmcl/server.pid");
            let raw = state(token)
                .replace("\"pid\":123", &format!("\"pid\":{}", child.0.id()))
                .replace(
                    "\"supervisor_pid\":456",
                    &format!("\"supervisor_pid\":{}", std::process::id()),
                );
            fs::write(&path, &raw).unwrap();
            let inode = file_identity(&path);
            let wall_ms = || {
                i64::try_from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis(),
                )
                .unwrap()
            };
            let start_ms = wall_ms();
            let mut response = request(binary, dir.path(), "server.status");
            let end_ms = wall_ms();
            let status = &response["result"];
            assert_eq!(status["running"], expected_started_at.is_some(), "{token}");
            assert_eq!(
                status["stale_state"],
                expected_started_at.is_none(),
                "{token}"
            );
            assert_eq!(
                status["started_at_ms"],
                json!(expected_started_at),
                "{token}"
            );
            if let Some(started_at_ms) = expected_started_at {
                let uptime = status["uptime_ms"].as_i64().expect("running uptime");
                let lower = (start_ms - started_at_ms).max(0);
                let upper = (end_ms - started_at_ms).max(0);
                assert!(
                    (lower..=upper).contains(&uptime),
                    "{token}: uptime {uptime} outside request interval {lower}..={upper}"
                );
            } else {
                assert!(status["uptime_ms"].is_null(), "{token}: stopped uptime");
            }
            // Normalize only after independently checking each request's elapsed
            // time against its wall-clock interval and explicit state expectation.
            response["result"]
                .as_object_mut()
                .unwrap()
                .remove("uptime_ms");
            let retained = path.exists();
            assert_eq!(retained, expected_started_at.is_some(), "{token}");
            if retained {
                assert_eq!(fs::read_to_string(&path).unwrap(), raw);
                assert_eq!(file_identity(&path), inode)
            }
            child.alive();
            results.push((response, retained));
        }
        assert_eq!(results[0], results[1], "state timestamp {token}");
        cases += 1;
    }
    for field in ["server_size", "launch_size", "java_major_version"] {
        for token in [
            "3",
            r#""3e0""#,
            r#""+0_3""#,
            r#""0x1.8p+1""#,
            r#""3.00000000000000000000000000000000001""#,
            "3.0000000000000001",
            r#""3.0000000000000001""#,
            r#""18446744073709551615.0""#,
            r#""18446744073709551615.1""#,
            "18446744073709551616",
            r#""65535e0""#,
            r#""65535.0000000000000001""#,
            "65536",
            r#""-0""#,
            r#""-1""#,
            r#""-+3""#,
            r#""0x1p0""#,
            "null",
            "true",
            "[]",
        ] {
            let mut results = vec![];
            for binary in [Some(oracle.as_str()), None] {
                let dir = tempfile::tempdir().unwrap();
                fixture(dir.path());
                let path = dir.path().join("demo/.jmcl/install.json");
                let raw = marker(field, token);
                fs::write(&path, &raw).unwrap();
                let inode = file_identity(&path);
                let jar = file_identity(&dir.path().join("shared.jar"));
                results.push(request(binary, dir.path(), "server.get"));
                assert_eq!(fs::read_to_string(&path).unwrap(), raw);
                assert_eq!(file_identity(&path), inode);
                assert_eq!(file_identity(&dir.path().join("demo/server.jar")), jar);
                assert_eq!(fs::read(dir.path().join("shared.jar")).unwrap(), b"abc");
                child.alive();
            }
            assert_eq!(results[0], results[1], "marker {field}={token}");
            cases += 1;
        }
    }
    // Requests deliberately retain dynamic Value/binary64 behavior. Reuse one
    // log inode so the native file_id does not need normalization.
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    fs::create_dir_all(dir.path().join("demo/logs")).unwrap();
    let log = dir.path().join("demo/logs/console.log");
    fs::write(&log, b"abcdef").unwrap();
    let inode = file_identity(&log);
    for field in ["max_bytes", "cursor", "file_id"] {
        let params = format!(
            r#"{{"directory":{},"id":"demo","{field}":3.0000000000000001}}"#,
            serde_json::to_string(dir.path()).unwrap()
        );
        let expected = request_params(Some(&oracle), "server.logs", &params);
        let actual = request_params(None, "server.logs", &params);
        assert_eq!(actual, expected, "dynamic {field}");
        assert_eq!(actual["event"], "result");
        assert_eq!(fs::read(&log).unwrap(), b"abcdef");
        assert_eq!(file_identity(&log), inode);
        child.alive();
        cases += 1;
    }
    eprintln!(
        "pinned persisted server record corpus: {cases} cases; no signals; all disk effects checked"
    );
}

#![cfg(unix)]
use jmcl_core_rust::launch::process::{self, Event};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsString,
    time::{Duration, Instant},
};
fn parent() -> process::Environment {
    HashMap::from(
        [
            ("HOME", "fixture-home"),
            ("FAKE_TOKEN", "fake-secret"),
            ("JAVA_TOOL_OPTIONS", "bad"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    )
}
fn run(mode: &str, extra: &[&str]) -> (Vec<Value>, Value) {
    let exe = std::env::var("JMCL_FAKE_LAUNCH_CHILD").unwrap();
    let mut args = vec![exe.as_str(), mode];
    args.extend_from_slice(extra);
    let mut logs = Vec::new();
    let result=process::run(&args,"/tmp",&["fake-secret"],&parent(),|e|if let Event::Log{stream,bytes,end}=e{logs.push(json!({"stream":stream,"bytes":String::from_utf8(bytes.to_vec()).unwrap(),"end":end}))}).unwrap();
    (logs, serde_json::to_value(result).unwrap())
}
#[test]
#[ignore = "requires native benign C fixture and pinned Zig process adapter"]
fn process_oracle() {
    for (mode, extra) in [
        ("emit", vec![]),
        ("large", vec![]),
        (
            "args",
            vec![
                "one argument",
                ";echo not-a-shell",
                "${literal}",
                "fake-secret",
            ],
        ),
        ("env", vec![]),
        ("signal", vec![]),
    ] {
        let (actual, r) = run(mode, &extra);
        let out =
            std::process::Command::new(std::env::var("JMCL_ZIG_LAUNCH_PROCESS_ORACLE").unwrap())
                .arg(std::env::var("JMCL_FAKE_LAUNCH_CHILD").unwrap())
                .arg(mode)
                .args(&extra)
                .output()
                .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut rows: Vec<Value> = out
            .stdout
            .split(|&c| c == b'\n')
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::from_slice(s).unwrap())
            .collect();
        let expected = rows.pop().unwrap();
        assert_eq!(actual, rows, "mode {mode}");
        for field in ["termination", "exit_code", "signal", "status"] {
            assert_eq!(r[field], expected[field], "mode {mode} field {field}")
        }
        assert!(r["pid"].as_u64().unwrap() > 0);
        assert!(r["duration_ms"].as_i64().unwrap() >= 0);
        assert!(r["max_rss"].as_u64().unwrap() > 0);
    }
}
/// ZIG-002 approved correction: bounded completion, not literal oracle equality.
/// Zig waits for descendant pipe EOF.
/// The sole synthetic descendant exits itself after 600ms; we never kill guessed PIDs.
#[test]
#[ignore = "requires native benign C fixture and pinned Zig process adapter"]
fn inherited_pipe_known_gap() {
    let start = Instant::now();
    let (logs, r) = run("descendant", &[]);
    assert!(start.elapsed() < Duration::from_millis(300));
    assert_eq!(r["exit_code"], 0);
    assert_eq!(logs[0]["bytes"], "parent done");
    let mut old =
        std::process::Command::new(std::env::var("JMCL_ZIG_LAUNCH_PROCESS_ORACLE").unwrap())
            .arg(std::env::var("JMCL_FAKE_LAUNCH_CHILD").unwrap())
            .arg("descendant")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        old.try_wait().unwrap().is_none(),
        "old Zig must still await inherited pipe EOF"
    );
    old.kill().unwrap();
    old.wait().unwrap();
    std::thread::sleep(Duration::from_millis(650));
}

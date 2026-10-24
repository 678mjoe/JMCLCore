//! Native host executable, including real PE on Windows. No shell interpolation.
use jmcl_core_rust::launch::process::{self, Event, RecordEnd, Stream};
#[test]
fn native_child_streams_literal_args_and_environment() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir
        .path()
        .join(if cfg!(windows) { "child.exe" } else { "child" });
    assert!(
        std::process::Command::new("rustc")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/launch/child.rs")
            )
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap()
            .success()
    );
    let mut parent: process::Environment = std::env::vars_os().collect();
    parent.insert("HOME".into(), "fixture-home".into());
    parent.insert("FAKE_TOKEN".into(), "fake-secret".into());
    parent.insert("JAVA_TOOL_OPTIONS".into(), "unsafe".into());
    let mut logs = vec![];
    let mut started = 0;
    let r = process::run(
        &[exe.to_str().unwrap(), "emit"],
        dir.path().to_str().unwrap(),
        &["fake-secret"],
        &parent,
        |e| match e {
            Event::Started { pid } => started = pid,
            Event::Log { stream, bytes, end } => logs.push((stream, bytes.to_vec(), end)),
        },
    )
    .unwrap();
    assert_eq!(r.pid, started);
    assert_eq!(r.termination, "exited");
    assert_eq!(r.exit_code, Some(7));
    assert!(r.max_rss.is_some());
    assert!(logs.contains(&(
        Stream::Stdout,
        b"out <redacted>".to_vec(),
        RecordEnd::Newline
    )));
    assert!(logs.contains(&(Stream::Stdout, vec![], RecordEnd::Newline)));
    assert!(logs.contains(&(Stream::Stderr, b"binary\0\xff".to_vec(), RecordEnd::Eof)));
    for (mode, extra, expected) in [
        (
            "args",
            vec!["one argument", ";not a shell", "${literal}"],
            vec!["one argument", ";not a shell", "${literal}"],
        ),
        (
            "env",
            vec![],
            vec!["HOME=fixture-home", "TOKEN=false", "JAVA=false"],
        ),
    ] {
        let mut args = vec![exe.to_str().unwrap(), mode];
        args.extend(extra);
        let mut lines = vec![];
        process::run(&args, dir.path().to_str().unwrap(), &[], &parent, |e| {
            if let Event::Log { bytes, .. } = e {
                lines.push(String::from_utf8(bytes.into()).unwrap())
            }
        })
        .unwrap();
        assert_eq!(lines, expected);
    }
}

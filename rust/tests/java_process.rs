//! Compile a real host executable, including PE on Windows, not a fake .exe shell script.
use jmcl_core_rust::java::detect::{self, CandidateSource};
use std::{
    process::Command,
    time::{Duration, Instant},
};
#[test]
fn native_probe_deadline_and_option_sanitization() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir
        .path()
        .join(if cfg!(windows) { "java.exe" } else { "java" });
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/java_probe.rs");
    assert!(
        Command::new("rustc")
            .arg(source)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let path = binary.to_str().unwrap();
    let mut env: detect::Environment = std::env::vars_os().collect();
    for k in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        env.insert(k.into(), "bad".into());
    }
    let runtime = detect::probe(path, CandidateSource::Explicit, &env).unwrap();
    assert_eq!(runtime.major_version, 17);
    assert_eq!(runtime.vendor.as_deref(), Some("Fixture"));
    for mode in ["hang", "hold"] {
        env.insert("JMCL_TEST_JAVA_MODE".into(), mode.into());
        let start = Instant::now();
        assert_eq!(
            detect::run_bounded(path, &[], &env, 256 * 1024, Duration::from_millis(100)),
            Err("Timeout")
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    env.insert("JMCL_TEST_JAVA_MODE".into(), "large".into());
    assert_eq!(
        detect::run_bounded(path, &[], &env, 1024, Duration::from_secs(1)),
        Err("StreamTooLong")
    );
    for _ in 0..3 {
        env.remove(std::ffi::OsStr::new("JMCL_TEST_JAVA_MODE"));
        assert!(detect::probe(path, CandidateSource::Explicit, &env).is_ok());
    }
}

#[test]
fn managed_receipt_preference_and_download_error_policy() {
    use jmcl_core_rust::java::{
        managed::{self, Policy, Request, Source},
        receipt, store,
    };
    let dir = tempfile::tempdir().unwrap();
    let platform =
        receipt::platform_name(managed::host_os().unwrap(), managed::host_arch().unwrap()).unwrap();
    let rt = dir.path().join("runtimes/mojang-fixture");
    std::fs::create_dir_all(rt.join("bin")).unwrap();
    let executable = if cfg!(windows) {
        "bin/java.exe"
    } else {
        "bin/java"
    };
    let java = rt.join(executable);
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/java_probe.rs");
    assert!(
        Command::new("rustc")
            .arg(source)
            .arg("-o")
            .arg(&java)
            .status()
            .unwrap()
            .success()
    );
    let doc = serde_json::json!({"provider":"mojang","id":"fixture","platform":platform,"major_version":17,"java_home":"","executable":executable,"source_url":"https://example.invalid/runtime","installed_at_ms":123,"component":"preferred"});
    let doc = receipt::parse(&serde_json::to_vec(&doc).unwrap()).unwrap();
    store::write_receipt(&rt, &doc).unwrap();
    let mut request = Request {
        required_major: 17,
        component: Some("preferred".into()),
        arch: managed::host_arch().unwrap().into(),
        policy: Policy::Managed,
        java_paths: vec![],
        java_override: None,
        store_directory: dir.path().to_str().unwrap().into(),
        allow_download: true,
    };
    let env = detect::Environment::new();
    let result = managed::resolve(Some(&env), &request, &mut |_, _| {
        panic!("installed receipt must win")
    })
    .unwrap();
    assert_eq!(result.source, Source::Managed);
    // An uninstalled major forces the injected downloader; no runtime is downloaded.
    request.required_major = 999;
    for (input, expected) in [
        ("Canceled", "Canceled"),
        ("OutOfMemory", "OutOfMemory"),
        ("NetworkError", "JavaDownloadFailed"),
    ] {
        assert_eq!(
            managed::resolve(Some(&env), &request, &mut |major, root| {
                assert_eq!(major, 999);
                assert_eq!(root, request.store_directory);
                Err(input)
            })
            .unwrap_err(),
            expected
        );
    }
    request.policy = Policy::Local;
    let result = managed::resolve(Some(&env), &request, &mut |_, _| {
        panic!("local policy forbids download")
    });
    assert!(matches!(
        result.unwrap_err(),
        "JavaNotFound" | "JavaIncompatible"
    ));
}

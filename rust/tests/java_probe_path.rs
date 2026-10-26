//! Unix PATH lookup with owned property-printing files only. No discovery,
//! system JVM, installer, runtime acquisition, archive or game operation.
#![cfg(unix)]
use jmcl_core_rust::java::detect::{self, CandidateSource};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

// Run in a separate process: changing the parent PATH of configure() must not
// race with other tests, rustc, or environment consumers in this test process.
#[test]
fn explicit_probe_child() {
    let Some(executable) = std::env::var_os("JMCL_PATH_PROBE_EXECUTABLE") else {
        return;
    };
    let output = std::env::var_os("JMCL_PATH_PROBE_OUTPUT").unwrap();
    let mut env = detect::Environment::new();
    env.insert("PATH".into(), "/jmcl-no-child-path".into());
    for name in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        env.insert(name.into(), "must-be-cleared".into());
    }
    let value = match detect::probe(
        executable.to_str().unwrap(),
        CandidateSource::Explicit,
        &env,
    ) {
        Ok(runtime) => json!({"result":runtime}),
        Err(error) => json!({"error":error}),
    };
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    fs::write(output, bytes).unwrap();
}
fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
#[test]
#[ignore = "requires Zig 0.16 compiler and exact pinned source; explicit synthetic probes only"]
fn unix_parent_path_search_matches_pinned_explicit_probe() {
    let source = std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required");
    let source = Path::new(&source).canonicalize().unwrap();
    let revision = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(revision.status.success());
    assert_eq!(
        String::from_utf8(revision.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    let compiler = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&compiler).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(version.stdout, b"0.16.0\n");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let adapter = root.join("oracle");
    let status = Command::new(&compiler)
        .args(["build-exe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/compat/java_probe_oracle.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            source.join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", adapter.display()))
        .status()
        .unwrap();
    assert!(status.success());
    let valid = root.join("valid");
    let denied = root.join("denied");
    let invalid = root.join("invalid");
    for d in [&valid, &denied, &invalid] {
        fs::create_dir(d).unwrap();
    }
    let name = "jmcl-owned-path-probe";
    let script=b"#!/bin/sh\n[ \"$PATH\" = /jmcl-no-child-path ] || exit 4\n[ -z \"$JAVA_TOOL_OPTIONS$_JAVA_OPTIONS$JDK_JAVA_OPTIONS\" ] || exit 3\nprintf 'java.specification.version = 17\\nos.arch = amd64\\nsun.arch.data.model = 64\\njava.vendor = OwnedFixture\\njava.home = /owned/fixture\\n' >&2\n";
    write(&valid.join(name), script, 0o755);
    write(&root.join(name), script, 0o755);
    write(&denied.join(name), script, 0o644);
    write(&invalid.join(name), b"not-an-executable", 0o755);
    write(&root.join("not-directory"), b"file", 0o644);
    let missing = root.join("missing");
    let non_dir = root.join("not-directory");
    let cases: Vec<(&str, String, Option<String>, Option<&str>)> = vec![
        (
            "absolute",
            valid.join(name).to_str().unwrap().into(),
            Some("".into()),
            None,
        ),
        (
            "relative-slash",
            format!("valid/{name}"),
            Some("".into()),
            None,
        ),
        (
            "bare-valid",
            name.into(),
            Some(valid.display().to_string()),
            None,
        ),
        (
            "missing-before-valid",
            name.into(),
            Some(format!("{}:{}", missing.display(), valid.display())),
            None,
        ),
        (
            "nondirectory-before-valid",
            name.into(),
            Some(format!("{}:{}", non_dir.display(), valid.display())),
            None,
        ),
        (
            "denied-before-valid",
            name.into(),
            Some(format!("{}:{}", denied.display(), valid.display())),
            None,
        ),
        (
            "denied-before-missing",
            name.into(),
            Some(format!("{}:{}", denied.display(), missing.display())),
            Some("AccessDenied"),
        ),
        (
            "missing-before-nondirectory",
            name.into(),
            Some(format!("{}:{}", missing.display(), non_dir.display())),
            Some("NotDir"),
        ),
        (
            "denied-before-nondirectory",
            name.into(),
            Some(format!("{}:{}", denied.display(), non_dir.display())),
            Some("AccessDenied"),
        ),
        (
            "nondirectory-before-missing",
            name.into(),
            Some(format!("{}:{}", non_dir.display(), missing.display())),
            Some("FileNotFound"),
        ),
        (
            "absolute-invalid-format",
            invalid.join(name).to_str().unwrap().into(),
            Some(valid.display().to_string()),
            Some("InvalidExe"),
        ),
        (
            "absolute-nondirectory",
            non_dir.join(name).to_str().unwrap().into(),
            Some(valid.display().to_string()),
            Some("NotDir"),
        ),
        (
            "empty-parent-path",
            name.into(),
            Some("".into()),
            Some("FileNotFound"),
        ),
        (
            "empty-path-components",
            name.into(),
            Some(":".into()),
            Some("FileNotFound"),
        ),
        (
            "absent-parent-path",
            name.into(),
            None,
            Some("FileNotFound"),
        ),
        (
            "invalid-before-valid",
            name.into(),
            Some(format!("{}:{}", invalid.display(), valid.display())),
            Some("InvalidExe"),
        ),
    ];
    let mut count = 0;
    let mut mismatches = Vec::new();
    for (label, executable, path, error) in cases {
        let output = root.join("rust-result.json");
        let mut rust = Command::new(std::env::current_exe().unwrap());
        rust.args(["--exact", "explicit_probe_child", "--nocapture"])
            .env("JMCL_PATH_PROBE_EXECUTABLE", &executable)
            .env("JMCL_PATH_PROBE_OUTPUT", &output)
            .current_dir(root);
        let mut zig = Command::new(&adapter);
        zig.arg(&executable).current_dir(root);
        for cmd in [&mut rust, &mut zig] {
            if let Some(path) = &path {
                cmd.env("PATH", path);
            } else {
                cmd.env_remove("PATH");
            }
        }
        let r = rust.output().unwrap();
        assert!(r.status.success(), "{label}: {:?}", r.stderr);
        let z = zig.output().unwrap();
        assert!(z.status.success(), "{label}: {:?}", z.stderr);
        let bytes = fs::read(&output).unwrap();
        if bytes != z.stdout {
            eprintln!(
                "{label}: Rust={} Zig={}",
                String::from_utf8_lossy(&bytes),
                String::from_utf8_lossy(&z.stdout)
            );
            mismatches.push(label);
        }
        let value: Value = serde_json::from_slice(&z.stdout).unwrap();
        if let Some(error) = error {
            assert_eq!(value, json!({"error":error}), "{label}");
        } else {
            let value = &value["result"];
            assert_eq!(value["executable"], executable);
            assert_eq!(value["major_version"], 17);
            assert_eq!(value["vendor"], "OwnedFixture");
            assert_eq!(value["source"], "explicit");
            assert_eq!(value["architecture"], "x86_64");
        }
        count += 1;
    }
    assert!(mismatches.is_empty(), "mismatched cases: {mismatches:?}");
    eprintln!("{count} exact Unix parent-PATH explicit probe cases; owned fixtures only");
}

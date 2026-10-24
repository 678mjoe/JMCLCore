//! Local/native execution only. Java runtime downloads are deliberately not attempted.
use jmcl_core_rust::{
    java::{detect, managed},
    launch::{self, executor, process::Event},
    metadata::{self, Source},
    version::{Service, Transport},
};
use serde_json::json;
use std::{collections::HashMap, path::Path};
struct Fixture {
    metadata: Vec<u8>,
}
impl Transport for Fixture {
    fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, metadata::Error> {
        if url.ends_with("version_manifest_v2.json") {
            Ok(json!({"latest":{"release":"fixture","snapshot":"fixture"},"versions":[{"id":"fixture","type":"release","url":"https://example.invalid/fixture","time":"2026-01-01","releaseTime":"2026-01-01"}]}).to_string().into_bytes())
        } else if url == "https://example.invalid/fixture" {
            Ok(self.metadata.clone())
        } else {
            Err(metadata::Error::Network)
        }
    }
}
fn input(directory: &str) -> executor::Input<'_> {
    executor::Input {
        id: "fixture",
        fabric_loader: None,
        neoforge_version: None,
        forge_version: None,
        directory,
        auth: launch::Auth {
            player_name: "Fixture",
            uuid: "fake-uuid",
            access_token: "fake-secret",
            session: None,
            user_type: "msa",
            xuid: "",
            client_id: "",
            user_properties: "{}",
        },
        options: launch::Options::default(),
        source: Source::Official,
        arch: None,
        java_paths: &[],
        java_policy: managed::Policy::Local,
        java_override: None,
        store_directory: "unused",
    }
}
fn children(root: &Path) -> usize {
    std::fs::read_dir(root.join(".jmcl/runtime/natives"))
        .unwrap()
        .count()
}
#[test]
fn native_launch_and_cleanup_on_all_owned_error_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("instance");
    let root_s = root.to_str().unwrap();
    let fixture=Fixture{metadata:json!({"id":"fixture","type":"release","mainClass":"emit","libraries":[],"downloads":{"client":{"sha1":"1".repeat(40),"size":0,"url":"https://example.invalid/client"}},"javaVersion":{"component":"fixture","majorVersion":17},"arguments":{"jvm":[],"game":[]}}).to_string().into_bytes()};
    let service = Service::new(fixture, tmp.path().join("cache"));
    let env: detect::Environment = std::env::vars_os().collect();
    let missing = executor::prepare(&service, input(root_s), None, |_| {
        panic!("no resolver without environment")
    });
    assert!(matches!(missing, Err("EnvironmentUnavailable")));
    assert_eq!(children(&root), 0);
    for error in ["JavaNotFound", "JavaIncompatible", "JavaDownloadFailed"] {
        let r = executor::prepare(&service, input(root_s), Some(&env), |request| {
            assert!(request.allow_download);
            assert_eq!(request.policy, managed::Policy::Local);
            Err(error)
        });
        assert!(matches!(r,Err(e)if e==error));
        assert_eq!(children(&root), 0)
    }
    let exe = tmp
        .path()
        .join(if cfg!(windows) { "child.exe" } else { "child" });
    assert!(
        std::process::Command::new("rustc")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/launch/child.rs"))
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap()
            .success()
    );
    let resolution = |path: &Path| managed::Resolution {
        runtime: detect::Runtime {
            executable: path.to_string_lossy().into_owned(),
            source: detect::CandidateSource::Explicit,
            major_version: 17,
            specification_version: "17".into(),
            version: None,
            runtime_version: None,
            vendor: None,
            java_home: None,
            vm_name: None,
            os_arch: "amd64".into(),
            os_version: None,
            architecture: Some("x86_64".into()),
            data_model: Some(64),
        },
        source: managed::Source::Forced,
    };
    let prepared = executor::prepare(&service, input(root_s), Some(&env), |_| {
        Ok(resolution(&exe))
    })
    .unwrap();
    assert_eq!(children(&root), 1);
    let mut logs = Vec::new();
    let result = prepared
        .run(&env, |event, _plan| {
            if let Event::Log { bytes, .. } = event {
                logs.extend_from_slice(bytes)
            }
        })
        .unwrap();
    assert_eq!(result.process.exit_code, Some(7));
    assert_eq!(result.native_cleanup, "removed");
    assert!(
        !logs
            .windows(b"fake-secret".len())
            .any(|b| b == b"fake-secret")
    );
    assert_eq!(children(&root), 0);
    let prepared = executor::prepare(&service, input(root_s), Some(&env), |_| {
        Ok(resolution(&tmp.path().join("missing")))
    })
    .unwrap();
    assert!(matches!(
        prepared.run(&HashMap::new(), |_, _| {}),
        Err("ProcessFailed")
    ));
    assert_eq!(children(&root), 0);
    let prepared = executor::prepare(&service, input(root_s), Some(&env), |_| {
        Ok(resolution(&exe))
    })
    .unwrap();
    drop(prepared);
    assert_eq!(children(&root), 0);
}

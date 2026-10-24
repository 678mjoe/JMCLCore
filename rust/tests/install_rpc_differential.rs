use jmcl_core_rust::{
    install::{
        self,
        service::{Planner, Selection},
    },
    metadata::{self, Source},
    plan::{Plan, Target},
    version,
};
use serde_json::{Value, json, value::RawValue};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
struct NoDownload;
impl install::Transport for NoDownload {
    fn fetch(&self, _: &str, _: &mut dyn Write) -> Result<u16, install::Error> {
        panic!("fixture must never download")
    }
}
struct NoMetadata;
impl version::Transport for NoMetadata {
    fn get(&self, _: &str, _: usize) -> Result<Vec<u8>, metadata::Error> {
        panic!("fixture cache miss")
    }
}
fn call(oracle: &str, cwd: &std::path::Path, input: &str) -> String {
    let mut p = Command::new(oracle)
        .arg("rpc")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    p.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = p.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha1::digest(b))
}
fn fixture(dir: &std::path::Path) -> version::Service<NoMetadata> {
    let metadata=json!({"id":"fixture","type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":"https://piston-data.mojang.com/fixture.jar","sha1":hash(b"abc"),"size":3}}}).to_string();
    let manifest=json!({"latest":{"release":"fixture","snapshot":"fixture"},"versions":[{"id":"fixture","type":"release","url":"https://piston-meta.mojang.com/fixture.json","sha1":hash(metadata.as_bytes()),"time":"now","releaseTime":"now"}]}).to_string();
    let cache = dir.join(".jmclcore/metadata-cache");
    fs::create_dir_all(&cache).unwrap();
    for (url, payload) in [
        (Source::Official.manifest_url(), manifest.clone()),
        (
            "https://piston-meta.mojang.com/fixture.json",
            metadata.clone(),
        ),
        (Source::Bmclapi.manifest_url(), manifest),
        ("https://bmclapi2.bangbang93.com/fixture.json", metadata),
    ] {
        let envelope = json!({"v":1,"url":url,"fetched_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),"payload":payload});
        fs::write(
            cache.join(format!("{}.json", hash(url.as_bytes()))),
            envelope.to_string(),
        )
        .unwrap();
    }
    for root in ["rust", "zig"] {
        fs::create_dir_all(dir.join(root).join("versions/fixture")).unwrap();
        fs::write(dir.join(root).join("versions/fixture/fixture.jar"), b"abc").unwrap();
    }
    version::Service::new(NoMetadata, cache)
}
#[test]
#[ignore = "requires pinned Zig oracle"]
fn install_parameter_schema_matches_without_network() {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let mut cases = vec![
        None,
        Some("null".into()),
        Some("{}".into()),
        Some("[]".into()),
        Some("{\"id\":null}".into()),
    ];
    for field in [
        "directory",
        "store_directory",
        "source",
        "os",
        "arch",
        "workers",
        "retries",
        "java_paths",
        "java_policy",
        "fabric_loader",
        "neoforge_version",
        "forge_version",
    ] {
        for value in [
            json!(null),
            json!(true),
            json!([]),
            json!(""),
            json!(0),
            json!("1"),
            json!(1.0),
            json!(256),
        ] {
            let mut p = json!({"id":""});
            p[field] = value;
            cases.push(Some(p.to_string()));
        }
    }
    let mut requests = String::new();
    let mut rust = Vec::new();
    for method in ["install.execute", "install.prepare"] {
        for (i, p) in cases.iter().enumerate() {
            let id = format!("{method}-{i}");
            let request = match p {
                Some(p) => format!(
                    "{{\"protocol\":1,\"id\":\"{id}\",\"method\":\"{method}\",\"params\":{p}}}\n"
                ),
                None => format!("{{\"protocol\":1,\"id\":\"{id}\",\"method\":\"{method}\"}}\n"),
            };
            requests.push_str(&request);
            let raw = p
                .as_ref()
                .map(|s| RawValue::from_string(s.clone()).unwrap());
            install::rpc::handle(&mut rust, &id, method, raw.as_deref())
                .unwrap()
                .unwrap();
        }
    }
    let zig = call(&oracle, tmp.path(), &requests);
    let rust = String::from_utf8(rust).unwrap();
    let differences: Vec<_> = rust
        .lines()
        .zip(zig.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert!(differences.is_empty(), "{differences:#?}");
}
#[test]
#[ignore = "requires pinned Zig oracle"]
fn cached_execute_receipt_and_prepare_envelopes_match() {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let service = fixture(tmp.path());
    for source in ["official", "bmclapi"] {
        for method in ["install.execute", "install.prepare"] {
            let zp = json!({"id":"fixture","source":source,"directory":tmp.path().join("zig"),"store_directory":tmp.path().join("zstore"),"workers":1,"retries":0});
            let rp = json!({"id":"fixture","source":source,"directory":tmp.path().join("rust"),"store_directory":tmp.path().join("rstore"),"workers":1,"retries":0});
            let input = json!({"protocol":1,"id":"fixture-request","method":method,"params":zp})
                .to_string()
                + "\n";
            let zig = call(&oracle, tmp.path(), &input);
            let raw = RawValue::from_string(rp.to_string()).unwrap();
            let mut rust = vec![];
            install::rpc::handle_with(
                &mut rust,
                "fixture-request",
                method,
                Some(&raw),
                &service,
                &NoDownload,
            )
            .unwrap()
            .unwrap();
            if method == "install.execute" {
                assert_eq!(String::from_utf8(rust).unwrap(), zig);
                assert_eq!(
                    fs::read(tmp.path().join("rust/.jmcl/install.json")).unwrap(),
                    fs::read(tmp.path().join("zig/.jmcl/install.json")).unwrap()
                );
            } else {
                let mut zv: Value = serde_json::from_str(zig.trim()).unwrap();
                let mut rv: Value = serde_json::from_slice(&rust).unwrap();
                for (v, root) in [(&mut zv, "zig"), (&mut rv, "rust")] {
                    let path = v["result"]["native_directory"].as_str().unwrap();
                    assert!(tmp.path().join(root).join(path).is_dir());
                    v["result"]["native_directory"] = json!("<native>");
                }
                assert_eq!(rv.to_string(), zv.to_string());
            }
        }
    }
}
#[test]
fn writer_failure_does_not_record_completed_installation() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let service = fixture(tmp.path());
    let raw=RawValue::from_string(json!({"id":"fixture","directory":tmp.path().join("rust"),"store_directory":tmp.path().join("store")}).to_string()).unwrap();
    assert!(
        install::rpc::handle_with(
            &mut Broken,
            "id",
            "install.execute",
            Some(&raw),
            &service,
            &NoDownload
        )
        .unwrap()
        .is_err()
    );
    assert!(!tmp.path().join("rust/.jmcl/install.json").exists());
}
#[test]
fn incomplete_processor_engine_never_records_success() {
    struct Forge;
    impl Planner for Forge {
        fn plan(&self, _: &Selection, target: Target) -> Result<Plan, metadata::Error> {
            Ok(Plan {
                version_id: "fixture".into(),
                source: Source::Official,
                target,
                files: vec![],
                classpath: vec![],
                native_extractions: vec![],
                asset_projections: vec![],
                total_download_size: 0,
                java_major_version: 17,
                loader: "forge".into(),
            })
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("game");
    let raw = RawValue::from_string(json!({"id":"fixture","directory":dir}).to_string()).unwrap();
    let mut out = vec![];
    install::rpc::handle_with(
        &mut out,
        "id",
        "install.execute",
        Some(&raw),
        &Forge,
        &NoDownload,
    )
    .unwrap()
    .unwrap();
    assert!(String::from_utf8(out).unwrap().contains("NOT_IMPLEMENTED"));
    assert!(!dir.exists());
}

#[test]
#[ignore = "requires pinned Zig oracle"]
fn install_cli_cached_execute_prepare_and_usage_match() {
    let oracle = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    for sub in ["execute", "prepare"] {
        let tmp = tempfile::tempdir().unwrap();
        let service = fixture(tmp.path());
        let args = |root: &str| {
            let mut a = vec![
                "install".to_string(),
                sub.into(),
                "fixture".into(),
                "--directory".into(),
                tmp.path().join(root).to_str().unwrap().into(),
                "--arch".into(),
                "amd64".into(),
            ];
            if sub == "execute" {
                a.extend([
                    "--store-directory".into(),
                    tmp.path()
                        .join(format!("{root}-store"))
                        .to_str()
                        .unwrap()
                        .into(),
                    "--workers".into(),
                    "0_1".into(),
                    "--retries".into(),
                    "-0".into(),
                ]);
            } else {
                a.extend([
                    "--forge-version".into(),
                    "intentionally-not-resolved".into(),
                ]);
            }
            a
        };
        let zig = Command::new(&oracle)
            .args(args("zig"))
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let mut out = vec![];
        let mut err = vec![];
        let code = install::cli::cli_with(&args("rust"), &mut out, &mut err, &service, &NoDownload)
            .unwrap()
            .unwrap();
        assert_eq!(i32::from(code), zig.status.code().unwrap());
        assert_eq!(err, zig.stderr);
        if sub == "execute" {
            assert_eq!(out, zig.stdout);
        } else {
            let normalize = |b: Vec<u8>| {
                String::from_utf8(b)
                    .unwrap()
                    .lines()
                    .map(|s| {
                        if s.starts_with("nativeDirectory\t") {
                            "nativeDirectory\t<native>"
                        } else {
                            s
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            assert_eq!(normalize(out), normalize(zig.stdout));
        }
        for tail in [
            vec![],
            vec![""],
            vec!["fixture", "--missing"],
            vec!["fixture", "--directory", ""],
            vec!["fixture", "--arch", "invalid"],
            vec!["fixture", "--source", "invalid"],
            vec!["fixture", "--workers", "33"],
            vec!["fixture", "--retries", "6"],
            vec!["fixture", "--workers", "_1"],
            vec!["fixture", "--workers", "1_"],
            vec!["fixture", "--java-policy", "invalid"],
        ] {
            let args = std::iter::once("install")
                .chain(std::iter::once(sub))
                .chain(tail)
                .map(str::to_string)
                .collect::<Vec<_>>();
            let zig = Command::new(&oracle)
                .args(&args)
                .current_dir(tmp.path())
                .output()
                .unwrap();
            let mut out = vec![];
            let mut err = vec![];
            let code = install::cli::cli_with(&args, &mut out, &mut err, &service, &NoDownload)
                .unwrap()
                .unwrap();
            assert_eq!(
                (i32::from(code), out, err),
                (zig.status.code().unwrap(), zig.stdout, zig.stderr),
                "{args:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires pinned Zig oracle"]
fn raw_rpc_directory_bytes_preserve_legacy_paths() {
    use std::os::unix::ffi::OsStrExt;
    let oracle = std::env::var("JMCL_ZIG_ORACLE").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let service = fixture(tmp.path());
    let raw_dir = |prefix: &str| {
        tmp.path().join(std::ffi::OsStr::from_bytes(
            &[prefix.as_bytes(), b"\xff"].concat(),
        ))
    };
    let rust = raw_dir("rust-");
    let zig = raw_dir("zig-");
    for root in [&rust, &zig] {
        fs::create_dir_all(root.join("versions/fixture")).unwrap();
        fs::write(root.join("versions/fixture/fixture.jar"), b"abc").unwrap();
    }
    let zp = json!({"id":"fixture","directory":zig.as_os_str().as_bytes(),"store_directory":tmp.path().join("zstore"),"workers":1});
    let rp = json!({"id":"fixture","directory":rust.as_os_str().as_bytes(),"store_directory":tmp.path().join("rstore"),"workers":1});
    let input =
        json!({"protocol":1,"id":"raw","method":"install.execute","params":zp}).to_string() + "\n";
    let zig_out = call(&oracle, tmp.path(), &input);
    let raw = RawValue::from_string(rp.to_string()).unwrap();
    let mut out = vec![];
    install::rpc::handle_with(
        &mut out,
        "raw",
        "install.execute",
        Some(&raw),
        &service,
        &NoDownload,
    )
    .unwrap()
    .unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), zig_out);
    assert!(rust.join(".jmcl/install.json").is_file());
}

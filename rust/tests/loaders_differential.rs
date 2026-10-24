//! Offline processor corpus: synthetic ZIPs and a benign native fake-Java only.
use jmcl_core_rust::{
    java::detect::Environment,
    loaders::{self, Loader, Options},
    store::{ArtifactRoot, Store},
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha1::digest(b))
}
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        z.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}
fn java() -> &'static Path {
    static JAVA: OnceLock<PathBuf> = OnceLock::new();
    JAVA.get_or_init(|| {
        let d = tempfile::tempdir().unwrap().keep();
        let exe = d.join(if cfg!(windows) { "java.exe" } else { "java" });
        assert!(
            Command::new("rustc")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loader_java.rs"))
                .arg("-o")
                .arg(&exe)
                .status()
                .unwrap()
                .success()
        );
        exe
    })
}
fn profile() -> Value {
    json!({"spec":1,"version":"fixture","minecraft":"1.20.4","data":{"PATCHED":{"client":"[a:patched:1:client]","server":"[a:patched:1:server]"},"PATCHED_SHA":{"client":format!("'{}'",hash(b"client")),"server":format!("'{}'",hash(b"server"))},"LITERAL":{"client":"'literal value'"},"BLOB":{"client":"/fixture.bin"}},"processors":[{"jar":"a:tool:1","args":["{PATCHED}","both","{LITERAL}","{BLOB}","[a:dep:1]"],"classpath":["a:dep:1"]},{"jar":"a:tool:1","sides":["client"],"args":["{PATCHED}","client","{LITERAL}","{BLOB}","[a:dep:1]"]},{"jar":"a:tool:1","sides":["server"],"args":["{PATCHED}","server","{LITERAL}","{BLOB}","[a:dep:1]"]}],"libraries":[]})
}
fn put(root: &Path, path: &str, bytes: &[u8]) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap()
}
struct Case {
    dir: tempfile::TempDir,
    loader: &'static str,
    scenario: &'static str,
}
impl Case {
    fn new(loader: &'static str, scenario: &'static str) -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            loader,
            scenario,
        }
    }
    fn server(&self) -> bool {
        self.loader.ends_with("server")
    }
    fn loader(&self) -> Loader<'_> {
        if self.loader.starts_with("forge") {
            Loader::Forge {
                minecraft: "1.20.4",
                version: "49.2.8",
            }
        } else {
            Loader::NeoForge {
                version: "21.1.133",
            }
        }
    }
    fn root(&self) -> PathBuf {
        #[cfg(unix)]
        if self.scenario == "nonutf8" {
            use std::os::unix::ffi::OsStringExt;
            return self
                .dir
                .path()
                .join(std::ffi::OsString::from_vec(b"root-\xff".to_vec()));
        }
        self.dir.path().join("root")
    }
    fn store(&self) -> PathBuf {
        self.dir.path().join("store")
    }
    fn support(&self) -> String {
        self.loader().support(self.server()).unwrap()
    }
    fn setup(&self) {
        let root = self.root();
        fs::create_dir(&root).unwrap();
        let mut p = profile();
        let mut manifest = b"Main-Class: fixture.Main\r\n".to_vec();
        let mut blob = true;
        let mut dep = true;
        let mut jar = true;
        match self.scenario {
            "failed" => p["processors"][0]["args"][1] = json!("fail"),
            "stdout-large" | "stderr-large" => p["processors"][0]["args"][1] = json!(self.scenario),
            "missing-output" => p["processors"] = json!([]),
            "no-op" => p["processors"] = json!([{"jar":"a:tool:1","args":["{PATCHED}","noop"]}]),
            "unknown" => p["processors"][0]["args"][0] = json!("{UNKNOWN}"),
            "unclosed" => p["processors"][0]["args"][0] = json!("{PATCHED"),
            "embedded-missing" => blob = false,
            "dependency-missing" => dep = false,
            "jar-missing" => jar = false,
            "manifest-missing" => manifest.clear(),
            "main-missing" => manifest = b"Manifest-Version: 1.0\n".to_vec(),
            "main-invalid" => manifest = b"Main-Class: bad/class\n".to_vec(),
            "main-continuation" => {
                manifest = b"Main-Class: fixture.\r\n Main\r\nOther: x\r\n".to_vec()
            }
            "hash-mismatch" => {
                p["data"]["PATCHED_SHA"][if self.server() { "server" } else { "client" }] =
                    json!(format!("'{}'", hash(b"other")))
            }
            "server-hash-missing" => {
                p["data"]["PATCHED_SHA"]
                    .as_object_mut()
                    .unwrap()
                    .remove("server");
            }
            "state-valid" | "state-stale" | "state-invalid" | "state-uppercase" => {
                let body = if self.scenario == "state-stale" {
                    b"stale".as_slice()
                } else {
                    b"cached"
                };
                let path = format!(
                    "libraries/a/patched/1/patched-1-{}.jar",
                    if self.server() { "server" } else { "client" }
                );
                put(&root, &path, body);
                let hash = if self.scenario == "state-uppercase" {
                    hash(b"cached").to_uppercase()
                } else {
                    hash(b"cached")
                };
                let state = if self.scenario == "state-invalid" {
                    b"not json".to_vec()
                } else {
                    format!("{{\"patched_sha1\":\"{hash}\",\"patched_size\":6}}\n").into_bytes()
                };
                put(&root, &format!("{}/state.json", self.support()), &state);
            }
            _ => {}
        }
        let p = serde_json::to_vec(&p).unwrap();
        let mut entries = vec![("install_profile.json", p.as_slice())];
        if blob {
            entries.push(("fixture.bin", b"embedded fixture"))
        }
        if self.scenario != "installer-missing" {
            let installer = if self.scenario == "profile-missing" {
                archive(&[])
            } else {
                archive(&entries)
            };
            put(
                &root,
                &format!("{}/installer.jar", self.support()),
                &installer,
            )
        }
        if jar {
            let entries = if manifest.is_empty() {
                vec![]
            } else {
                vec![("META-INF/MANIFEST.MF", manifest.as_slice())]
            };
            put(&root, "libraries/a/tool/1/tool-1.jar", &archive(&entries));
        }
        if dep {
            put(&root, "libraries/a/dep/1/dep-1.jar", b"not executed")
        }
    }
    fn rust(&self) -> Vec<Value> {
        let root = ArtifactRoot::open(self.root()).unwrap();
        let store = Store::open(self.store()).unwrap();
        let opts = Options {
            server: self.server(),
            java_paths: vec![java().to_string_lossy().into_owned()],
            required_major: 17,
            ..Default::default()
        };
        let mut out = Vec::new();
        let r = loaders::run(
            &root,
            &store,
            self.loader(),
            "1.20.4",
            &opts,
            Some(&Environment::new()),
            &mut |p| {
                out.push(json!({"processor":p}));
                Ok(())
            },
        );
        out.push(match r {
            Ok(s) => serde_json::to_value(s).unwrap(),
            Err(e) => json!({"error":e.name()}),
        });
        out
    }
    fn zig(&self, exe: &std::ffi::OsStr) -> Vec<Value> {
        let out = Command::new(exe)
            .args([self.loader])
            .arg(self.root())
            .arg(self.store())
            .arg(java())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    fn snapshot(&self) -> Value {
        let root = self.root();
        json!({"state":fs::read_to_string(root.join(self.support()).join("state.json")).ok(),"order":fs::read_to_string(root.join("processor-order.txt")).ok(),"work_exists":root.join(self.support()).join("work").exists(),"patched":fs::read_to_string(root.join(format!("libraries/a/patched/1/patched-1-{}.jar",if self.server(){"server"}else{"client"}))).ok()})
    }
}
#[test]
fn synthetic_native_chain_and_state() {
    for mode in ["neo", "neo-server", "forge", "forge-server"] {
        let c = Case::new(mode, "success");
        c.setup();
        let out = c.rust();
        assert_eq!(out.last().unwrap()["status"], "patched", "{out:?}");
        assert_eq!(out.last().unwrap()["processors_run"], 2);
        let expected = if c.server() { "server" } else { "client" };
        let patched = c
            .root()
            .join(format!("libraries/a/patched/1/patched-1-{expected}.jar"));
        assert_eq!(fs::read(patched).unwrap(), expected.as_bytes());
        assert_eq!(
            out.last().unwrap()["patched_sha1"],
            hash(expected.as_bytes())
        );
        assert_eq!(out.last().unwrap()["patched_size"], expected.len());
        let state: Value = serde_json::from_slice(
            &fs::read(c.root().join(c.support()).join("state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["patched_sha1"], hash(expected.as_bytes()));
        assert_eq!(state["patched_size"], expected.len());
        assert!(!c.root().join(c.support()).join("work").exists());
        let out = c.rust();
        assert_eq!(out.last().unwrap()["status"], "cached");
        assert_eq!(out.len(), 1);
    }
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_LOADERS_ORACLE"]
fn exact_processor_oracle() {
    let exe =
        std::env::var_os("JMCL_ZIG_LOADERS_ORACLE").expect("required pinned processor oracle");
    let mut windows_mismatches = Vec::new();
    for mode in ["neo", "neo-server", "forge", "forge-server"] {
        for scenario in [
            "success",
            "failed",
            "stdout-large",
            "stderr-large",
            "missing-output",
            "no-op",
            "unknown",
            "unclosed",
            "embedded-missing",
            "dependency-missing",
            "jar-missing",
            "manifest-missing",
            "main-missing",
            "main-invalid",
            "main-continuation",
            "hash-mismatch",
            "server-hash-missing",
            "state-valid",
            "state-stale",
            "state-invalid",
            "state-uppercase",
            "installer-missing",
            "profile-missing",
            "nonutf8",
        ] {
            let rust = Case::new(mode, scenario);
            if scenario == "nonutf8" {
                #[cfg(unix)]
                match fs::create_dir(rust.root()) {
                    Ok(()) => fs::remove_dir(rust.root()).unwrap(),
                    Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => {
                        eprintln!(
                            "NOT_EXERCISED {mode}/nonutf8 success: filesystem rejects raw path with EILSEQ; not a processor parity pass"
                        );
                        continue;
                    }
                    Err(error) => panic!("unexpected raw-root capability error: {error:?}"),
                }
                #[cfg(not(unix))]
                {
                    eprintln!(
                        "NOT_EXERCISED {mode}/nonutf8 success: Unix byte-path fixture is not supported on this target"
                    );
                    continue;
                }
            }
            rust.setup();
            let zig = Case::new(mode, scenario);
            zig.setup();
            let actual = rust.rust();
            let baseline = zig.zig(&exe);
            let actual_disk = rust.snapshot();
            let baseline_disk = zig.snapshot();
            if let Some(expected) = zig008_expected("engine", mode, scenario, None) {
                let observed = json!({"mode":mode,"scenario":scenario,"rust":actual,"zig":baseline,
                    "rust_disk":actual_disk,"zig_disk":baseline_disk});
                zig008_check_rust(expected, &observed);
                zig008_check_files(&rust, expected, false);
                if zig008_enabled() && (actual != baseline || actual_disk != baseline_disk) {
                    assert!(
                        zig008_matches(expected, &observed),
                        "ZIG008 unapproved engine evidence: {observed}"
                    );
                    zig008_check_absence(&zig, &expected["zig_disk"]);
                    eprintln!("ZIG008_APPROVED engine {mode}/{scenario}");
                    continue;
                }
            }
            if cfg!(windows)
                && std::env::var("JMCL_DIAGNOSE_WINDOWS_LOADER_MATRIX").as_deref() == Ok("1")
            {
                // Collect the entire bounded corpus, but NEVER accept a mismatch.
                // No known-defect allowance is implemented here.
                eprintln!(
                    "WINDOWS_LOADER_MATRIX {}",
                    json!({
                        "mode":mode,"scenario":scenario,"rust":actual,"zig":baseline,
                        "rust_disk":actual_disk,"zig_disk":baseline_disk
                    })
                );
                if actual != baseline || actual_disk != baseline_disk {
                    windows_mismatches.push(format!("{mode}/{scenario}"));
                    continue;
                }
            }
            assert_eq!(
                serde_json::to_string(&actual).unwrap(),
                serde_json::to_string(&baseline).unwrap(),
                "{mode}/{scenario}"
            );
            assert_eq!(actual_disk, baseline_disk, "disk {mode}/{scenario}");
        }
    }
    assert!(
        windows_mismatches.is_empty(),
        "strict Windows loader mismatches: {windows_mismatches:?}"
    );
}
#[test]
fn reporter_failure_leaves_no_state() {
    let c = Case::new("neo", "success");
    c.setup();
    let root = ArtifactRoot::open(c.root()).unwrap();
    let store = Store::open(c.store()).unwrap();
    let error = loaders::run(
        &root,
        &store,
        c.loader(),
        "1.20.4",
        &Options {
            java_paths: vec![java().to_string_lossy().into_owned()],
            ..Default::default()
        },
        Some(&Environment::new()),
        &mut |_| Err(jmcl_core_rust::install::Error::named("WriterFailed")),
    )
    .unwrap_err();
    assert_eq!(error.name(), "WriterFailed");
    assert!(!c.root().join(c.support()).join("state.json").exists());
    assert!(!c.root().join(c.support()).join("work").exists());
}

#[test]
fn cached_output_needs_no_environment_and_is_hardlinked() {
    let c = Case::new("neo", "success");
    c.setup();
    let first = c.rust();
    let summary = first.last().unwrap();
    assert_eq!(
        summary["status"], "patched",
        "cache precondition failed: {first:?}"
    );
    let root = ArtifactRoot::open(c.root()).unwrap();
    let store = Store::open(c.store()).unwrap();
    let second = loaders::run(
        &root,
        &store,
        c.loader(),
        "1.20.4",
        &Options::default(),
        None,
        &mut |_| panic!("cached chain emits no processor events"),
    )
    .unwrap();
    assert_eq!(second.status, loaders::Status::Cached);
    let patched = c.root().join("libraries/a/patched/1/patched-1-client.jar");
    let hash = summary["patched_sha1"].as_str().unwrap();
    let object = c.store().join("objects/sha1").join(&hash[..2]).join(hash);
    assert_eq!(fs::read(&object).unwrap(), fs::read(&patched).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&object).unwrap().ino(),
            fs::metadata(&patched).unwrap().ino()
        );
        assert!(fs::metadata(&object).unwrap().nlink() >= 2);
    }
}
#[test]
fn uncached_output_requires_environment() {
    let c = Case::new("neo", "success");
    c.setup();
    let root = ArtifactRoot::open(c.root()).unwrap();
    let store = Store::open(c.store()).unwrap();
    let error = loaders::run(
        &root,
        &store,
        c.loader(),
        "1.20.4",
        &Options::default(),
        None,
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(error.name(), "EnvironmentUnavailable");
    assert!(!c.root().join(c.support()).join("state.json").exists());
}
#[test]
fn processor_java_major_is_preference_then_at_least_eight() {
    use loaders::Runtime;
    let mut r = loaders::SystemRuntime::default();
    let executable = r
        .resolve(
            &Environment::new(),
            &[java().to_string_lossy().into_owned()],
            99,
            "arm",
        )
        .unwrap();
    assert_eq!(executable, java().to_string_lossy());
}

struct NoNetwork;
impl jmcl_core_rust::version::Transport for NoNetwork {
    fn get(&self, _: &str, _: usize) -> Result<Vec<u8>, jmcl_core_rust::metadata::Error> {
        panic!("fixture metadata cache miss")
    }
}
impl jmcl_core_rust::install::Transport for NoNetwork {
    fn fetch(&self, _: &str, _: &mut dyn Write) -> Result<u16, jmcl_core_rust::install::Error> {
        panic!("fixture attempted download")
    }
}
fn install_fixture(c: &Case, legacy: bool) -> jmcl_core_rust::version::Service<NoNetwork> {
    use jmcl_core_rust::{
        install::service::{Planner, Selection},
        metadata::Source,
        plan::Target,
        version,
    };
    let id = if c.loader == "forge" {
        "1.20.4-forge-49.2.8"
    } else {
        "neoforge-21.1.133"
    };
    let old = fs::read(c.root().join(c.support()).join("installer.jar")).unwrap();
    let mut ip: Value = serde_json::from_slice(
        &version::zip::read_entry(&old, "install_profile.json", 16 * 1024 * 1024)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    ip["version"] = json!(id);
    let mut vp = json!({"id":id,"inheritsFrom":"1.20.4","mainClass":"fixture.Main","libraries":[]});
    if legacy {
        ip["data"] = json!({});
        ip["processors"] = json!([]);
        vp["libraries"] = json!([{"name":"a:legacy:1","downloads":{"artifact":{"path":"a/legacy/1/legacy-1.jar","url":"https://example.invalid/legacy.jar","sha1":hash(b"abc"),"size":3}}}]);
    }
    if c.scenario == "legacy" {
        ip = json!({"install":{"path":"a:legacy:1","minecraft":"1.20.4"},"versionInfo":{"id":id,"inheritsFrom":"1.20.4","mainClass":"fixture.Main","minecraftArguments":"fixture","libraries":[{"name":"a:legacy:1","url":"https://maven.minecraftforge.net/","checksums":[hash(b"abc")]}]}});
    }
    let ip = serde_json::to_vec(&ip).unwrap();
    let vp = serde_json::to_vec(&vp).unwrap();
    let installer = archive(&[
        ("install_profile.json", &ip),
        ("version.json", &vp),
        ("fixture.bin", b"embedded fixture"),
    ]);
    put(
        &c.root(),
        &format!("{}/installer.jar", c.support()),
        &installer,
    );
    let metadata=json!({"id":"1.20.4","type":"release","mainClass":"fixture.Main","javaVersion":{"majorVersion":if c.scenario=="java-incompatible"{99}else{17},"component":"java-runtime-gamma"},"libraries":[],"downloads":{"client":{"url":"https://example.invalid/client.jar","sha1":hash(b"abc"),"size":3}}}).to_string();
    let url = "https://example.invalid/metadata.json";
    let manifest=json!({"latest":{"release":"1.20.4","snapshot":"1.20.4"},"versions":[{"id":"1.20.4","type":"release","url":url,"sha1":hash(metadata.as_bytes()),"time":"now","releaseTime":"now"}]}).to_string();
    let installer_url = if c.loader == "forge" {
        "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-installer.jar"
    } else {
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.133/neoforge-21.1.133-installer.jar"
    };
    let cache = c.dir.path().join(".jmclcore/metadata-cache");
    fs::create_dir_all(&cache).unwrap();
    for (url, payload) in [
        (Source::Official.manifest_url(), json!(manifest)),
        (url, json!(metadata)),
        (installer_url, json!(installer)),
        (
            "head:https://maven.minecraftforge.net/a/legacy/1/legacy-1-universal.jar",
            json!("3"),
        ),
    ] {
        let envelope = json!({"v":1,"url":url,"fetched_at":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),"payload":payload});
        fs::write(
            cache.join(format!("{}.json", hash(url.as_bytes()))),
            envelope.to_string(),
        )
        .unwrap();
    }
    let service = version::Service::new(NoNetwork, cache);
    let selection = Selection {
        id: "1.20.4".into(),
        fabric_loader: None,
        neoforge_version: (c.loader == "neo").then(|| "21.1.133".into()),
        forge_version: (c.loader == "forge").then(|| "49.2.8".into()),
        source: Source::Official,
        os: None,
        arch: None,
    };
    let plan = Planner::plan(&service, &selection, Target::host().unwrap()).unwrap();
    assert_eq!(
        plan.loader,
        if legacy {
            "forge_legacy"
        } else if c.loader == "neo" {
            "neoforge"
        } else {
            "forge"
        }
    );
    for file in plan.files {
        let bytes = if file.sha1 == hash(&installer) {
            &installer[..]
        } else {
            assert_eq!(file.sha1, hash(b"abc"));
            b"abc"
        };
        put(&c.root(), &file.path, bytes)
    }
    service
}
fn install_params(c: &Case) -> Value {
    let mut p = json!({"id":"1.20.4","directory":c.root(),"store_directory":c.store(),"java_paths":[java()],"java_policy":"local","workers":1});
    p[if c.loader == "forge" {
        "forge_version"
    } else {
        "neoforge_version"
    }] = json!(if c.loader == "forge" {
        "49.2.8"
    } else {
        "21.1.133"
    });
    p
}
#[test]
#[ignore = "requires network-disabled pinned Zig protocol adapter"]
fn install_processor_protocol_matches_oracle() {
    use jmcl_core_rust::install;
    let mut windows_mismatches = Vec::new();
    let exe = std::env::var_os("JMCL_ZIG_LOADERS_INSTALL_ORACLE")
        .expect("JMCL_ZIG_LOADERS_INSTALL_ORACLE required");
    for mode in ["neo", "forge"] {
        for scenario in [
            "success",
            "failed",
            "missing-output",
            "hash-mismatch",
            "java-incompatible",
            "processorless",
            "legacy",
        ] {
            if mode == "neo" && matches!(scenario, "processorless" | "legacy") {
                continue;
            }
            let rust = Case::new(mode, scenario);
            rust.setup();
            let service = install_fixture(&rust, matches!(scenario, "processorless" | "legacy"));
            let zig = Case::new(mode, scenario);
            zig.setup();
            let _ = install_fixture(&zig, matches!(scenario, "processorless" | "legacy"));
            for run in 0..2 {
                let params =
                    serde_json::value::RawValue::from_string(install_params(&rust).to_string())
                        .unwrap();
                let mut out = Vec::new();
                install::rpc::handle_with(
                    &mut out,
                    "fixture",
                    "install.execute",
                    Some(&params),
                    &service,
                    &NoNetwork,
                )
                .unwrap()
                .unwrap();
                let input=json!({"protocol":1,"id":"fixture","method":"install.execute","params":install_params(&zig)}).to_string();
                let result = Command::new(&exe)
                    .arg(input)
                    .current_dir(zig.dir.path())
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let rust_text = String::from_utf8(out).unwrap();
                let zig_text = String::from_utf8(result.stdout).unwrap();
                if let Some(expected) = zig008_expected("rpc", mode, scenario, Some(run)) {
                    let observed = json!({"mode":mode,"scenario":scenario,"run":run,"rust":rust_text,"zig":zig_text,
                        "rust_disk":rust.snapshot(),"zig_disk":zig.snapshot(),
                        "rust_receipt":receipt(&rust),"zig_receipt":receipt(&zig)});
                    zig008_check_rust(expected, &observed);
                    zig008_check_files(&rust, expected, true);
                    if zig008_enabled()
                        && (rust_text != zig_text
                            || rust.snapshot() != zig.snapshot()
                            || receipt(&rust) != receipt(&zig))
                    {
                        assert!(
                            zig008_matches(expected, &observed),
                            "ZIG008 unapproved RPC evidence: {observed}"
                        );
                        zig008_check_absence(&zig, &expected["zig_disk"]);
                        zig008_assert_absent(&zig.root().join(".jmcl/install.json"));
                        eprintln!("ZIG008_APPROVED rpc {mode}/{scenario}/run{run}");
                        continue;
                    }
                }
                if cfg!(windows)
                    && std::env::var("JMCL_DIAGNOSE_WINDOWS_LOADER_MATRIX").as_deref() == Ok("1")
                {
                    let rust_receipt = fs::read(rust.root().join(".jmcl/install.json")).ok();
                    let zig_receipt = fs::read(zig.root().join(".jmcl/install.json")).ok();
                    eprintln!(
                        "WINDOWS_LOADER_RPC_MATRIX {}",
                        json!({
                            "mode":mode,"scenario":scenario,"run":run,
                            "rust":rust_text,"zig":zig_text,
                            "rust_disk":rust.snapshot(),"zig_disk":zig.snapshot(),
                            "rust_receipt":rust_receipt,"zig_receipt":zig_receipt
                        })
                    );
                    if rust_text != zig_text || rust_receipt != zig_receipt {
                        windows_mismatches.push(format!("{mode}/{scenario}/run{run}"));
                        continue;
                    }
                }
                if std::env::var_os("JMCL_ALLOW_PROCESSOR_HASH_LIFETIME_GAP").is_some()
                    && !matches!(scenario, "processorless" | "legacy")
                {
                    let mut zig_records: Vec<Value> = zig_text
                        .lines()
                        .map(|l| serde_json::from_str(l).unwrap())
                        .collect();
                    let rust_records: Vec<Value> = rust_text
                        .lines()
                        .map(|l| serde_json::from_str(l).unwrap())
                        .collect();
                    if let Some(last) = zig_records.last_mut()
                        && last["event"] == "result"
                    {
                        let actual = hash(
                            &fs::read(
                                rust.root()
                                    .join("libraries/a/patched/1/patched-1-client.jar"),
                            )
                            .unwrap(),
                        );
                        assert_eq!(
                            rust_records.last().unwrap()["result"]["patched_client"],
                            actual
                        );
                        let value = &mut last["result"]["patched_client"];
                        if *value != json!(actual) {
                            assert_eq!(
                                *value,
                                json!(vec![170u8; 40]),
                                "ZIG-003: only the observed Debug freed-memory pattern is allowed; Rust must return the actual output hash"
                            );
                            *value = json!(actual);
                        }
                    }
                    assert_eq!(
                        rust_text,
                        zig_records
                            .iter()
                            .map(|v| format!("{v}\n"))
                            .collect::<String>(),
                        "ZIG-003 {mode}/{scenario}/run{run}: only the approved patched_client lifetime correction may differ"
                    );
                } else {
                    assert_eq!(rust_text, zig_text, "{mode}/{scenario}/run{run}");
                }
                assert_eq!(
                    fs::read(rust.root().join(".jmcl/install.json")).ok(),
                    fs::read(zig.root().join(".jmcl/install.json")).ok(),
                    "receipt {mode}/{scenario}/run{run}"
                );
            }
        }
    }
    assert!(
        windows_mismatches.is_empty(),
        "strict Windows loader RPC mismatches: {windows_mismatches:?}"
    );
}

#[test]
#[ignore = "requires network-disabled pinned Zig CLI adapter"]
fn install_processor_cli_matches_oracle() {
    use jmcl_core_rust::install;
    let exe = std::env::var_os("JMCL_ZIG_LOADERS_INSTALL_ORACLE")
        .expect("JMCL_ZIG_LOADERS_INSTALL_ORACLE required");
    for mode in ["neo", "forge"] {
        for scenario in [
            "success",
            "missing-output",
            "hash-mismatch",
            "java-incompatible",
            "processorless",
            "legacy",
        ] {
            if mode == "neo" && matches!(scenario, "processorless" | "legacy") {
                continue;
            }
            let rust = Case::new(mode, scenario);
            rust.setup();
            let service = install_fixture(&rust, matches!(scenario, "processorless" | "legacy"));
            let zig = Case::new(mode, scenario);
            zig.setup();
            let _ = install_fixture(&zig, matches!(scenario, "processorless" | "legacy"));
            let args = |c: &Case| {
                vec![
                    "install".into(),
                    "execute".into(),
                    "1.20.4".into(),
                    "--directory".into(),
                    c.root().to_string_lossy().into_owned(),
                    "--store-directory".into(),
                    c.store().to_string_lossy().into_owned(),
                    if mode == "forge" {
                        "--forge-version".into()
                    } else {
                        "--neoforge-version".into()
                    },
                    if mode == "forge" {
                        "49.2.8".into()
                    } else {
                        "21.1.133".into()
                    },
                    "--java-path".into(),
                    java().to_string_lossy().into_owned(),
                    "--java-policy".into(),
                    "local".into(),
                    "--workers".into(),
                    "1".into(),
                ]
            };
            for run in 0..2 {
                let mut out = Vec::new();
                let mut err = Vec::new();
                let status =
                    install::cli::cli_with(&args(&rust), &mut out, &mut err, &service, &NoNetwork)
                        .unwrap()
                        .unwrap();
                let result = Command::new(&exe)
                    .args(args(&zig))
                    .env("JMCL_FIXTURE_JAVA_BIN", java().parent().unwrap())
                    .current_dir(zig.dir.path())
                    .output()
                    .unwrap();
                assert_eq!(
                    status as i32,
                    result.status.code().unwrap(),
                    "{mode}/{scenario}/{run}"
                );
                assert_eq!(
                    String::from_utf8(out).unwrap(),
                    String::from_utf8(result.stdout).unwrap(),
                    "stdout {mode}/{scenario}/{run}"
                );
                assert_eq!(
                    String::from_utf8(err).unwrap(),
                    String::from_utf8(result.stderr).unwrap(),
                    "stderr {mode}/{scenario}/{run}"
                );
                assert_eq!(
                    fs::read(rust.root().join(".jmcl/install.json")).ok(),
                    fs::read(zig.root().join(".jmcl/install.json")).ok()
                );
            }
        }
    }
}

#[test]
fn processor_event_writer_failure_never_records_installation() {
    use jmcl_core_rust::install;
    struct BrokenAtProcessor;
    impl Write for BrokenAtProcessor {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if b.windows(b"processor_started".len())
                .any(|w| w == b"processor_started")
            {
                Err(std::io::ErrorKind::BrokenPipe.into())
            } else {
                Ok(b.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let c = Case::new("neo", "success");
    c.setup();
    let service = install_fixture(&c, false);
    let params = serde_json::value::RawValue::from_string(install_params(&c).to_string()).unwrap();
    assert!(
        install::rpc::handle_with(
            &mut BrokenAtProcessor,
            "fixture",
            "install.execute",
            Some(&params),
            &service,
            &NoNetwork
        )
        .unwrap()
        .is_err()
    );
    assert!(!c.root().join(".jmcl/install.json").exists());
    assert!(!c.root().join(c.support()).join("state.json").exists());
    assert!(!c.root().join(c.support()).join("work").exists());
}
#[test]
fn unavailable_managed_acquisition_is_explicit_failure() {
    use jmcl_core_rust::install;
    let c = Case::new("neo", "java-incompatible");
    c.setup();
    let service = install_fixture(&c, false);
    let mut params = install_params(&c);
    params["java_policy"] = json!("auto");
    let params = serde_json::value::RawValue::from_string(params.to_string()).unwrap();
    let mut out = Vec::new();
    install::rpc::handle_with(
        &mut out,
        "fixture",
        "install.execute",
        Some(&params),
        &service,
        &NoNetwork,
    )
    .unwrap()
    .unwrap();
    let records: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        records.last().unwrap()["error"]["code"],
        "JAVA_DOWNLOAD_FAILED"
    );
    assert!(!c.root().join(".jmcl/install.json").exists());
    assert!(!c.root().join(c.support()).join("state.json").exists());
}

#[test]
fn native_fixture_containment_uses_directory_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let cwd = temporary.path().join("root");
    fs::create_dir(&cwd).unwrap();
    let child = cwd.join("child");
    fs::create_dir(&child).unwrap();
    let sibling = temporary.path().join("root-other");
    fs::create_dir(&sibling).unwrap();
    let embedded = cwd.join("blob");
    fs::write(&embedded, b"embedded fixture").unwrap();
    for (parent, success) in [(&child, true), (&sibling, false)] {
        let output = fs::canonicalize(parent).unwrap().join("output.jar");
        let result = Command::new(java())
            .args(["-cp", "fixture", "fixture.Main"])
            .arg(&output)
            .args(["client", "literal value"])
            .arg(&embedded)
            .arg("dep-1.jar")
            .current_dir(&cwd)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if success {
            assert_eq!(fs::read(&output).unwrap(), b"client");
        } else {
            assert!(!output.exists());
        }
    }
}
#[test]
#[cfg(windows)]
#[ignore = "requires pinned Windows Zig path diagnostic"]
fn windows_verbatim_join_diagnostic() {
    let exe =
        std::env::var_os("JMCL_ZIG_LOADERS_ORACLE").expect("JMCL_ZIG_LOADERS_ORACLE required");
    let temporary = tempfile::tempdir().unwrap();
    let out = Command::new(exe)
        .arg("--path-probe")
        .arg(temporary.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observed: Value = serde_json::from_slice(&out.stdout).unwrap();
    eprintln!("Windows pinned loader path evidence: {observed}");
    assert_eq!(observed["mixed"], "BadPathName");
    assert_eq!(observed["native"], "ok");
}

// These finite rows are reviewed native Windows observations. Their Rust side is
// independently checked against the pinned Linux oracle by the strict corpus.
fn zig008_rows(kind: &str) -> &'static Vec<Value> {
    static ROWS: OnceLock<Value> = OnceLock::new();
    ROWS.get_or_init(|| serde_json::from_str(include_str!("fixtures/loaders/zig008.json")).unwrap())
        [kind]
        .as_array()
        .unwrap()
}
fn zig008_expected(
    kind: &str,
    mode: &str,
    scenario: &str,
    run: Option<usize>,
) -> Option<&'static Value> {
    zig008_rows(kind).iter().find(|row| {
        row["mode"] == mode
            && row["scenario"] == scenario
            && row.get("run").and_then(Value::as_u64) == run.map(|n| n as u64)
    })
}
fn zig008_matches(expected: &Value, observed: &Value) -> bool {
    // Compare every field, including exact serialized record and state order.
    let a = expected.as_object().unwrap();
    let Some(b) = observed.as_object() else {
        return false;
    };
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            b.get(key).is_some_and(|actual| {
                serde_json::to_vec(value).unwrap() == serde_json::to_vec(actual).unwrap()
            })
        })
}
fn zig008_enabled() -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
        && std::env::var("JMCL_ALLOW_ZIG008_LOADER_PATHS").as_deref() == Ok("1")
}
// Receipt target fields are intentionally platform-dependent. Substitute only
// those two expected fields when validating the contract on a different host;
// the Windows deviation matcher still uses the untouched native evidence.
fn zig008_host_receipt(expected: &Value) -> Value {
    let Some(text) = expected.as_str() else {
        return expected.clone();
    };
    let mut receipt: Value = serde_json::from_str(text).unwrap();
    assert_eq!(receipt["os"], "windows");
    assert_eq!(receipt["arch"], "x86_64");
    let target = jmcl_core_rust::plan::Target::host().unwrap();
    receipt["os"] = json!(target.os);
    receipt["arch"] = json!(target.arch);
    json!(format!("{receipt}\n"))
}
fn zig008_check_rust(expected: &Value, observed: &Value) {
    for key in ["rust", "rust_disk", "rust_receipt"] {
        assert_eq!(
            expected
                .get(key)
                .map(|v| serde_json::to_vec(&if key == "rust_receipt" {
                    zig008_host_receipt(v)
                } else {
                    v.clone()
                })
                .unwrap()),
            observed.get(key).map(|v| serde_json::to_vec(v).unwrap()),
            "ZIG008 pinned-Linux contract {}/{}/{} field {key}",
            observed["mode"],
            observed["scenario"],
            observed["run"]
        );
    }
}
fn receipt(c: &Case) -> Option<String> {
    fs::read_to_string(c.root().join(".jmcl/install.json")).ok()
}
fn zig008_assert_absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        other => panic!("expected absent entry {path:?}, got {other:?}"),
    }
}
fn zig008_check_absence(c: &Case, disk: &Value) {
    let root = c.root();
    for (field, path) in [
        ("state", root.join(c.support()).join("state.json")),
        ("order", root.join("processor-order.txt")),
        (
            "patched",
            root.join(format!(
                "libraries/a/patched/1/patched-1-{}.jar",
                if c.server() { "server" } else { "client" }
            )),
        ),
    ] {
        if disk[field].is_null() {
            zig008_assert_absent(&path);
        }
    }
    let work = root.join(c.support()).join("work");
    if disk["work_exists"].as_bool().unwrap() {
        assert!(fs::symlink_metadata(work).unwrap().is_dir());
    } else {
        zig008_assert_absent(&work);
    }
}
fn zig008_check_files(c: &Case, expected: &Value, rpc: bool) {
    assert_eq!(c.snapshot(), expected["rust_disk"]);
    zig008_check_absence(c, &expected["rust_disk"]);
    zig008_assert_absent(&c.root().join(c.support()).join("work"));
    let state = &expected["rust_disk"]["state"];
    if let Some(state) = state.as_str() {
        let state: Value = serde_json::from_str(state).unwrap();
        let expected_bytes = expected["rust_disk"]["patched"]
            .as_str()
            .unwrap()
            .as_bytes();
        let digest = hash(expected_bytes);
        assert_eq!(state["patched_sha1"], digest);
        assert_eq!(state["patched_size"], expected_bytes.len());
        let patched = c.root().join(format!(
            "libraries/a/patched/1/patched-1-{}.jar",
            if c.server() { "server" } else { "client" }
        ));
        let object = c
            .store()
            .join("objects/sha1")
            .join(&digest[..2])
            .join(&digest);
        assert_eq!(fs::read(&patched).unwrap(), expected_bytes);
        assert_eq!(fs::read(&object).unwrap(), expected_bytes);
        assert_shared_file(&object, &patched);
    } else {
        zig008_assert_absent(&c.root().join(c.support()).join("state.json"));
        zig008_assert_absent(&c.root().join(".jmcl/install.json"));
    }
    if rpc {
        assert_eq!(
            json!(receipt(c)),
            zig008_host_receipt(&expected["rust_receipt"])
        );
    }
}
fn assert_shared_file(object: &Path, destination: &Path) {
    for path in [object, destination] {
        assert!(fs::metadata(path).unwrap().permissions().readonly());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let a = fs::metadata(object).unwrap();
        let b = fs::metadata(destination).unwrap();
        assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
        assert!(a.nlink() >= 2);
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let identity = |path: &Path| {
            let file = fs::File::open(path).unwrap();
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            assert_ne!(
                unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) },
                0
            );
            assert!(info.nNumberOfLinks >= 2);
            (
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
            )
        };
        assert_eq!(identity(object), identity(destination));
    }
}
#[test]
fn zig008_matcher_rejects_unlisted_or_mutated_evidence() {
    assert_eq!(zig008_rows("engine").len(), 75);
    assert_eq!(zig008_rows("rpc").len(), 12);
    fn leaves(value: &Value, path: String, out: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    leaves(child, format!("{path}/{key}"), out);
                }
            }
            Value::Array(array) => {
                for (index, child) in array.iter().enumerate() {
                    leaves(child, format!("{path}/{index}"), out);
                }
            }
            _ => out.push(path),
        }
    }
    for mode in ["neo", "neo-server", "forge", "forge-server"] {
        for scenario in [
            "state-valid",
            "state-uppercase",
            "installer-missing",
            "profile-missing",
        ] {
            assert!(zig008_expected("engine", mode, scenario, None).is_none());
        }
    }
    assert!(zig008_expected("engine", "forge-server", "server-hash-missing", None).is_none());
    for kind in ["engine", "rpc"] {
        let mut seen = std::collections::BTreeSet::new();
        assert!(zig008_expected(kind, "unknown", "success", None).is_none());
        assert!(zig008_expected(kind, "neo", "unlisted-scenario", None).is_none());
        for row in zig008_rows(kind) {
            assert!(seen.insert(format!(
                "{}/{}/{}",
                row["mode"], row["scenario"], row["run"]
            )));
            assert!(zig008_matches(row, row));
            let mut paths = Vec::new();
            leaves(row, String::new(), &mut paths);
            for path in paths {
                let mut changed = row.clone();
                let leaf = changed.pointer_mut(&path).unwrap();
                *leaf = match leaf {
                    Value::String(text) => json!(format!("{text}!")),
                    Value::Bool(value) => json!(!*value),
                    Value::Number(value) => json!(value.as_u64().unwrap() + 1),
                    _ => json!("unexpected"),
                };
                assert!(!zig008_matches(row, &changed), "{kind} leaf {path}");
            }
            // Every named case must reject changes in mode, scenario, run,
            // events/errors/hashes, disk state/order, and receipt bytes.
            for key in row.as_object().unwrap().keys() {
                let mut changed = row.clone();
                changed[key] = json!({"unexpected":"mutation"});
                assert!(!zig008_matches(row, &changed), "{kind} {key}");
                let mut missing = row.clone();
                missing.as_object_mut().unwrap().remove(key);
                assert!(!zig008_matches(row, &missing), "missing {kind} {key}");
            }
            let mut extra = row.clone();
            extra["unexpected"] = json!(true);
            assert!(!zig008_matches(row, &extra));
        }
    }
}

#[test]
fn zig008_cas_invariant_rejects_private_or_writable_files() {
    let temporary = tempfile::tempdir().unwrap();
    let object = temporary.path().join("object");
    let copy = temporary.path().join("copy");
    fs::write(&object, b"client").unwrap();
    fs::write(&copy, b"client").unwrap();
    let shared = temporary.path().join("shared");
    fs::hard_link(&object, &shared).unwrap();
    // Both independent inodes have >=2 links; reject by identity, not just count.
    fs::hard_link(&copy, temporary.path().join("private-shared")).unwrap();
    assert!(std::panic::catch_unwind(|| assert_shared_file(&object, &shared)).is_err());
    for path in [&object, &copy] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    }
    assert_shared_file(&object, &shared);
    assert!(std::panic::catch_unwind(|| assert_shared_file(&object, &copy)).is_err());
}

#[test]
fn zig008_absence_rejects_existing_or_dangling_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("entry");
    zig008_assert_absent(&path);
    fs::write(&path, b"not absent").unwrap();
    assert!(std::panic::catch_unwind(|| zig008_assert_absent(&path)).is_err());
    fs::remove_file(&path).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temporary.path().join("missing-target"), &path).unwrap();
        assert!(std::panic::catch_unwind(|| zig008_assert_absent(&path)).is_err());
    }
}

//! Existing managed-candidate lookup only, never resolve() or its fallback paths.
//! All receipts and the native property printer are owned temporary fixtures.
use super::*;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _temp: tempfile::TempDir,
    store: PathBuf,
    printer: PathBuf,
}
impl Fixture {
    fn executable() -> &'static str {
        if cfg!(windows) {
            "bin/probe.exe"
        } else {
            "bin/probe"
        }
    }
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("store");
        fs::create_dir_all(store.join("runtimes")).unwrap();
        let printer = temp.path().join(if cfg!(windows) {
            "printer.exe"
        } else {
            "printer"
        });
        let output = Command::new("rustc")
            .arg("--edition=2024")
            .arg("--deny=warnings")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/java_probe.rs"))
            .arg("-o")
            .arg(&printer)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        Self {
            _temp: temp,
            store,
            printer,
        }
    }
    fn request(&self) -> Request {
        Request {
            required_major: 17,
            component: Some("preferred".into()),
            arch: host_arch().unwrap().into(),
            policy: Policy::Managed,
            java_paths: vec![],
            java_override: None,
            store_directory: self.store.to_str().unwrap().into(),
            allow_download: false,
        }
    }
    fn document(&self) -> Value {
        json!({
            "provider":"mojang", "id":"fixture",
            "platform":receipt::platform_name(host_os().unwrap(), host_arch().unwrap()).unwrap(),
            "major_version":17, "java_home":"", "executable":Self::executable(),
            "source_url":"https://fixture.invalid/metadata-only", "installed_at_ms":123,
            "component":"preferred"
        })
    }
    fn write(&self, name: &str, document: &Value) -> PathBuf {
        let runtime = self.store.join("runtimes").join(name);
        fs::create_dir_all(runtime.join("bin")).unwrap();
        let executable = runtime.join(Self::executable());
        if !executable.exists() {
            fs::copy(&self.printer, &executable).unwrap();
        }
        fs::write(
            runtime.join(receipt::NAME),
            serde_json::to_vec(document).unwrap(),
        )
        .unwrap();
        executable
    }
    fn select(&self, request: &Request) -> Option<Runtime> {
        // An explicit empty environment disables all inherited fixture modes.
        // Calling this private branch cannot reach local discovery or a downloader.
        managed(&Environment::new(), request)
    }
}

#[test]
fn listed_receipt_representations_select_the_owned_existing_runtime() {
    let fixture = Fixture::new();
    let ordinary = fixture.document();
    let mut cases = vec![("ordinary", ordinary.clone())];
    for (field, value) in [
        ("provider", json!(0)),
        ("provider", json!("+0")),
        ("provider", json!(1)),
        ("id", json!(b"fixture".to_vec())),
        (
            "platform",
            json!(ordinary["platform"].as_str().unwrap().as_bytes()),
        ),
        ("executable", json!(Fixture::executable().as_bytes())),
        ("java_home", json!([])),
        ("component", json!(b"preferred".to_vec())),
        ("version", json!(["49", 55.0])),
        ("vendor", json!([255, 0, 9])),
        ("package_uuid", json!([255])),
        ("package_name", json!([255])),
        (
            "source_url",
            json!(b"https://fixture.invalid/metadata-only".to_vec()),
        ),
    ] {
        let mut document = ordinary.clone();
        document[field] = value;
        cases.push((field, document));
    }
    for (label, document) in cases {
        let executable = fixture.write("owned", &document);
        let original = fs::read(fixture.store.join("runtimes/owned").join(receipt::NAME)).unwrap();
        let listed = store::list_read(fixture.store.to_str().unwrap()).unwrap();
        assert_eq!(listed.len(), 1, "listing {label}");
        let selected = fixture
            .select(&fixture.request())
            .unwrap_or_else(|| panic!("listed but not selected: {label}"));
        assert_eq!(selected.executable, executable.to_str().unwrap(), "{label}");
        assert_eq!(selected.source, CandidateSource::Managed);
        assert_eq!(selected.major_version, 17);
        assert_eq!(selected.architecture.as_deref(), host_arch());
        assert_eq!(
            fs::read(fixture.store.join("runtimes/owned").join(receipt::NAME)).unwrap(),
            original
        );
    }
}

#[test]
fn byte_components_preserve_preference_and_sorted_fallback() {
    let fixture = Fixture::new();
    let mut generic = fixture.document();
    generic["component"] = json!([255]);
    let first = fixture.write("a-generic", &generic);
    let mut preferred = fixture.document();
    preferred["component"] = json!(b"preferred".to_vec());
    let favored = fixture.write("z-preferred", &preferred);
    let mut request = fixture.request();
    assert_eq!(
        fixture.select(&request).unwrap().executable,
        favored.to_str().unwrap()
    );
    request.component = None;
    assert_eq!(
        fixture.select(&request).unwrap().executable,
        first.to_str().unwrap()
    );
    request.component = Some("absent".into());
    assert_eq!(
        fixture.select(&request).unwrap().executable,
        first.to_str().unwrap()
    );
    request.required_major = 99;
    assert!(fixture.select(&request).is_none());
    request.required_major = 17;
    request.arch = "unsupported".into();
    assert!(fixture.select(&request).is_none());
}

#[test]
fn invalid_receipts_do_not_become_candidates() {
    let fixture = Fixture::new();
    for (field, value) in [
        ("provider", json!(2)),
        ("platform", json!("unsupported")),
        ("major_version", json!(0)),
        ("executable", json!("../probe")),
        ("vendor", json!([256])),
    ] {
        let mut document = fixture.document();
        document[field] = value;
        fixture.write("owned", &document);
        assert!(
            store::list_read(fixture.store.to_str().unwrap())
                .unwrap()
                .is_empty(),
            "{field}"
        );
        assert!(fixture.select(&fixture.request()).is_none(), "{field}");
    }
}

#[test]
fn opaque_executable_remains_a_listed_unresolved_candidate() {
    let fixture = Fixture::new();
    let mut document = fixture.document();
    document["executable"] = json!([98, 105, 110, 47, 255]);
    fixture.write("owned", &document);
    let listed = store::list_read(fixture.store.to_str().unwrap()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].receipt.executable.0, b"bin/\xff");
    assert!(fixture.select(&fixture.request()).is_none());
}

#[test]
fn opaque_executable_does_not_hide_a_later_eligible_candidate() {
    let fixture = Fixture::new();
    let mut opaque = fixture.document();
    opaque["executable"] = json!([98, 105, 110, 47, 255]);
    fixture.write("a-opaque", &opaque);
    let later = fixture.write("z-valid", &fixture.document());
    assert_eq!(
        store::list_read(fixture.store.to_str().unwrap())
            .unwrap()
            .len(),
        2
    );
    let selected = fixture.select(&fixture.request()).unwrap();
    assert_eq!(selected.executable, later.to_str().unwrap());
}

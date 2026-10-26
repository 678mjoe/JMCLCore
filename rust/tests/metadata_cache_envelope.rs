//! Persisted cache only: injected transport records a call and always fails.
//! The oracle invokes exported metadata_cache.lookup directly, never Service.
use jmcl_core_rust::{
    metadata::Error,
    version::{Policy, Service, Transport},
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
const URL: &str = "https://fixture.invalid/cache";
struct Reject(Cell<usize>);
impl Transport for Reject {
    fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, Error> {
        assert_eq!(url, URL);
        self.0.set(self.0.get() + 1);
        Err(Error::Network)
    }
}
fn identity(path: &Path) -> (u64, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(path).unwrap();
        (m.dev(), m.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let f = fs::File::open(path).unwrap();
        let mut i = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        assert_ne!(
            unsafe { GetFileInformationByHandle(f.as_raw_handle(), i.as_mut_ptr()) },
            0
        );
        let i = unsafe { i.assume_init() };
        (
            u64::from(i.dwVolumeSerialNumber),
            (u64::from(i.nFileIndexHigh) << 32) | u64::from(i.nFileIndexLow),
        )
    }
}
fn key(root: &Path) -> PathBuf {
    root.join(format!("{:x}.json", Sha1::digest(URL.as_bytes())))
}
fn rust(root: &Path, now: i64, max: usize, ttl: Option<u64>) -> Value {
    let mut service = Service::new(Reject(Cell::new(0)), root);
    service.now = now;
    let result = service.cached(URL, max, ttl.map_or(Policy::Immutable, Policy::Ttl));
    match result {
        Ok((payload, cached)) => {
            assert!(cached);
            json!({"payload":payload,"fresh":service.transport.0.get()==0})
        }
        Err(error) => {
            assert!(matches!(error, Error::Network));
            assert_eq!(service.transport.0.get(), 1);
            Value::Null
        }
    }
}
fn envelope(v: &str, time: &str, url: &str, payload: &str) -> Vec<u8> {
    format!(r#"{{"v":{v},"url":{url},"fetched_at":{time},"payload":{payload}}}"#).into_bytes()
}
#[test]
fn typed_cache_envelope_keeps_valid_cache_without_transport() {
    let root = tempfile::tempdir().unwrap();
    let bytes = envelope(
        r#""1.0""#,
        r#""+1__0""#,
        &serde_json::to_string(&URL.as_bytes()).unwrap(),
        r#"["65",66.0,255]"#,
    );
    fs::write(key(root.path()), &bytes).unwrap();
    assert_eq!(
        rust(root.path(), 10, 16, Some(5)),
        json!({"payload":[65,66,255],"fresh":true})
    );
    assert_eq!(fs::read(key(root.path())).unwrap(), bytes);
}
fn build_oracle(executable: &Path) {
    let source =
        PathBuf::from(std::env::var_os("JMCL_ZIG_SOURCE").expect("pinned source required"));
    let head = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
    let version = Command::new(&zig).arg("version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
    let output = Command::new(zig)
        .arg("build-exe")
        .args(["-O", "ReleaseSafe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/metadata_cache.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            source.join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", executable.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and Zig 0.16.0; cache operations only"]
fn persisted_envelopes_match_pinned_cache_and_disk_effects() {
    let temp = tempfile::tempdir().unwrap();
    let oracle = temp
        .path()
        .join(if cfg!(windows) { "cache.exe" } else { "cache" });
    build_oracle(&oracle);
    let url = serde_json::to_string(URL).unwrap();
    type Case = (String, Vec<u8>, i64, usize, Option<u64>);
    let mut cases: Vec<Case> = Vec::new();
    for token in [
        "1",
        "1.0",
        r#""1""#,
        r#""+0__1""#,
        r#""1e0""#,
        r#""0x1.0p0""#,
        "1.0000000000000001",
        r#""1.0000000000000001""#,
        "4294967295",
        "4294967296",
        "-1",
        "true",
        "null",
        "[]",
        "{}",
    ] {
        cases.push((
            format!("v {token}"),
            envelope(token, "10", &url, r#""AB""#),
            10,
            16,
            Some(5),
        ));
    }
    for token in [
        "10",
        "10.0",
        r#""+1__0""#,
        r#""1e1""#,
        "10.0000000000000001",
        r#""10.0000000000000001""#,
        "-1",
        "-0.0",
        r#""-+1""#,
        r#""+-1""#,
        "9223372036854775808",
        "-9223372036854775809",
        "true",
        "null",
        "[]",
        "{}",
    ] {
        cases.push((
            format!("timestamp {token}"),
            envelope("1", token, &url, r#""AB""#),
            10,
            16,
            Some(5),
        ));
    }
    // Safe equal now values avoid the source's unchecked signed subtraction.
    for token in [
        "9223372036854775807",
        r#""9223372036854775807.0""#,
        "-9223372036854775808",
        r#""-9223372036854775808.0""#,
    ] {
        let now = if token.contains('-') {
            i64::MIN
        } else {
            i64::MAX
        };
        cases.push((
            format!("signed boundary {token}"),
            envelope("1", token, &url, r#""AB""#),
            now,
            16,
            Some(0),
        ));
    }
    for token in [
        r#""AB""#,
        "[65,66]",
        r#"["65",66.0]"#,
        r#"["+6__5",255]"#,
        "[0,255]",
        "[]",
        "[65.0000000000000001]",
        "[256]",
        "[-1]",
        "[true]",
        "null",
        "{}",
    ] {
        cases.push((
            format!("payload {token}"),
            envelope("1", "10", &url, token),
            10,
            16,
            Some(5),
        ));
    }
    for token in [
        url.clone(),
        serde_json::to_string(URL.as_bytes()).unwrap(),
        serde_json::to_string(&URL.bytes().map(|b| b.to_string()).collect::<Vec<_>>()).unwrap(),
        "[255]".into(),
        r#""https://wrong.invalid""#.into(),
        "null".into(),
    ] {
        cases.push((
            format!("url {token}"),
            envelope("1", "10", &token, r#""AB""#),
            10,
            16,
            Some(5),
        ));
    }
    for (now, ttl) in [
        (9, Some(0)),
        (10, Some(0)),
        (15, Some(5)),
        (16, Some(5)),
        (86410, Some(5)),
        (86411, Some(5)),
        (86411, None),
    ] {
        cases.push((
            format!("age {now}/{ttl:?}"),
            envelope("1", "10", &url, r#""AB""#),
            now,
            16,
            ttl,
        ));
    }
    for max in [0, 1, 2] {
        cases.push((
            format!("size {max}"),
            envelope("1", "10", &url, r#""AB""#),
            10,
            max,
            Some(0),
        ));
    }
    for bytes in [
        b"{".to_vec(),
        envelope("2", "10", &url, r#""AB""#),
        format!(
            "{} ",
            String::from_utf8(envelope("1", "10", &url, r#""AB""#)).unwrap()
        )
        .into_bytes(),
    ] {
        cases.push(("shape".into(), bytes, 10, 16, Some(5)));
    }
    for unknown in [
        r#"1e400"#,
        r#"{"nested":["valid\ud83d\ude00"]}"#,
        r#""\ud800""#,
        r#""\udc00""#,
        r#""\\ud800""#,
    ] {
        let doc =
            format!(r#"{{"v":1,"url":{url},"fetched_at":10,"payload":"AB","unknown":{unknown}}}"#);
        cases.push((
            format!("unknown {unknown}"),
            doc.into_bytes(),
            10,
            16,
            Some(5),
        ));
    }
    let mut bad_utf8 =
        format!(r#"{{"v":1,"url":{url},"fetched_at":10,"payload":"AB","unknown":"X"}}"#)
            .into_bytes();
    let offset = bad_utf8.iter().rposition(|b| *b == b'X').unwrap();
    bad_utf8[offset] = 255;
    cases.push(("invalid JSON UTF8".into(), bad_utf8, 10, 16, Some(5)));
    for (index, (name, bytes, now, max, ttl)) in cases.iter().enumerate() {
        let mut results = Vec::new();
        for subject in ["zig", "rust"] {
            let root = temp.path().join(format!("{index}-{subject}"));
            fs::create_dir(&root).unwrap();
            let entry = key(&root);
            fs::write(&entry, bytes).unwrap();
            let peer = root.join("peer");
            fs::hard_link(&entry, &peer).unwrap();
            let sibling = root.join("sibling");
            fs::write(&sibling, b"unchanged").unwrap();
            let before = fs::metadata(&peer).unwrap();
            let before_id = identity(&peer);
            let sibling_id = identity(&sibling);
            let sibling_modified = fs::metadata(&sibling).unwrap().modified().unwrap();
            let value = if subject == "rust" {
                rust(&root, *now, *max, *ttl)
            } else {
                let output = Command::new(&oracle)
                    .arg(&root)
                    .arg(now.to_string())
                    .arg(max.to_string())
                    .arg(ttl.map_or("immutable".into(), |n| n.to_string()))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{name}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                serde_json::from_slice(&output.stdout).unwrap()
            };
            let retained = entry.try_exists().unwrap();
            assert_eq!(retained, !value.is_null(), "{name}/{subject}");
            assert_eq!(fs::read(&peer).unwrap(), *bytes, "peer {name}/{subject}");
            assert_eq!(fs::read(&sibling).unwrap(), b"unchanged");
            assert_eq!(
                fs::metadata(&peer).unwrap().permissions().readonly(),
                before.permissions().readonly()
            );
            assert_eq!(
                fs::metadata(&peer).unwrap().modified().unwrap(),
                before.modified().unwrap()
            );
            assert_eq!(
                fs::metadata(&sibling).unwrap().modified().unwrap(),
                sibling_modified
            );
            assert_eq!(identity(&peer), before_id);
            assert_eq!(identity(&sibling), sibling_id);
            if retained {
                assert_eq!(identity(&entry), before_id);
            }

            if retained {
                assert_eq!(fs::read(&entry).unwrap(), *bytes);
            }
            assert_eq!(
                fs::read_dir(&root).unwrap().count(),
                if retained { 3 } else { 2 }
            );
            results.push((value, retained));
        }
        assert_eq!(results[0], results[1], "{name}");
    }
    eprintln!(
        "{} persisted cache cases matched; rejecting fake transport only; peer/sibling preservation checked",
        cases.len()
    );
}

#[test]
fn cache_freshness_boundaries_and_failed_refresh_are_explicit() {
    for (now, ttl, expected, retained) in [
        (9, Some(0), json!({"payload":[65,66],"fresh":true}), true),
        (15, Some(5), json!({"payload":[65,66],"fresh":true}), true),
        (16, Some(5), json!({"payload":[65,66],"fresh":false}), true),
        (
            86410,
            Some(5),
            json!({"payload":[65,66],"fresh":false}),
            true,
        ),
        (86411, Some(5), Value::Null, false),
        (86411, None, json!({"payload":[65,66],"fresh":true}), true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bytes = envelope("1", "10", &serde_json::to_string(URL).unwrap(), r#""AB""#);
        fs::write(key(root.path()), &bytes).unwrap();
        assert_eq!(rust(root.path(), now, 2, ttl), expected);
        assert_eq!(key(root.path()).try_exists().unwrap(), retained);
        if retained {
            assert_eq!(fs::read(key(root.path())).unwrap(), bytes);
        }
    }
}

//! Only owned opaque files and an injected in-memory ASCII-URL transport.
//! No HTTP clients, archives, resolvers, Java or executable processes.
use super::*;
use crate::plan::{File as PlannedFile, Target, Usage};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
struct Fake {
    bodies: Mutex<Vec<Result<Vec<u8>, Error>>>,
    calls: Mutex<Vec<String>>,
}
impl Fake {
    fn bodies(bodies: Vec<Result<Vec<u8>, Error>>) -> Self {
        Self {
            bodies: Mutex::new(bodies),
            calls: Mutex::new(vec![]),
        }
    }
}
impl Transport for Fake {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, Error> {
        assert!(matches!(
            url,
            "https://fixture.invalid/opaque" | "https://fixture.invalid/fallback"
        ));
        self.calls.lock().unwrap().push(url.into());
        let body = self.bodies.lock().unwrap().remove(0)?;
        sink.write_all(&body).map_err(Error::from)?;
        Ok(200)
    }
}
fn file(path: &[u8], body: &[u8]) -> ByteFile {
    ByteFile {
        path: BytePath(path.to_vec()),
        url: "https://fixture.invalid/opaque".into(),
        fallback_url: None,
        sha1: format!("{:x}", Sha1::digest(body)),
        size: body.len() as u64,
    }
}
fn options() -> Options {
    Options {
        workers: 1,
        retries: 0,
        sync_files: true,
    }
}
fn native(root: &Path, path: &[u8]) -> PathBuf {
    root.join(crate::store::path_from_bytes(path).unwrap())
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
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        assert_ne!(
            unsafe { GetFileInformationByHandle(f.as_raw_handle(), info.as_mut_ptr()) },
            0
        );
        let i = unsafe { info.assume_init() };
        (
            u64::from(i.dwVolumeSerialNumber),
            (u64::from(i.nFileIndexHigh) << 32) | u64::from(i.nFileIndexLow),
        )
    }
}
fn byte_event(e: ByteEvent) -> Value {
    match e {
        ByteEvent::File(v) => json!({"file":v}),
        ByteEvent::Retry(v) => json!({"retry":v}),
        ByteEvent::Progress(v) => json!({"progress":v}),
    }
}
fn public_event(e: Event) -> Value {
    match e {
        Event::File(v) => json!({"file":v}),
        Event::Retry(v) => json!({"retry":v}),
        Event::Progress(v) => json!({"progress":v}),
    }
}
fn assert_shared(root: &Path, cache: &Path, file: &ByteFile, body: &[u8]) {
    let path = native(root, &file.path.0);
    let object = cache.join(crate::store::object_path(&file.sha1).unwrap());
    assert_eq!(fs::read(&path).unwrap(), body);
    assert_eq!(fs::read(&object).unwrap(), body);
    assert_eq!(identity(&path), identity(&object));
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    assert!(fs::metadata(&object).unwrap().permissions().readonly());
}
#[test]
fn byte_execution_matches_string_events_and_hardlinks() {
    let body = b"opaque immutable bytes";
    let f = file("mods/unicode-猫.jar".as_bytes(), body);
    let temp = tempfile::tempdir().unwrap();
    let mut results = vec![];
    for raw in [false, true] {
        let root_path = temp.path().join(if raw { "bytes" } else { "string" });
        let cache_path = temp
            .path()
            .join(if raw { "byte-cache" } else { "string-cache" });
        let root = ArtifactRoot::open(&root_path).unwrap();
        let store = Store::open(&cache_path).unwrap();
        let transport = Fake::bodies(vec![Ok(body.to_vec())]);
        let events = Mutex::new(vec![]);
        let plan = Plan {
            version_id: "fixture".into(),
            source: crate::metadata::Source::Official,
            target: Target::host().unwrap(),
            files: vec![PlannedFile {
                path: String::from_utf8(f.path.0.clone()).unwrap(),
                url: f.url.clone(),
                fallback_url: None,
                sha1: f.sha1.clone(),
                size: f.size,
                usage: Usage::default(),
            }],
            classpath: vec![],
            native_extractions: vec![],
            asset_projections: vec![],
            total_download_size: f.size,
            java_major_version: 0,
            loader: "vanilla".into(),
        };
        let summary = if raw {
            execute_bytes(
                &root,
                &store,
                std::slice::from_ref(&f),
                f.size,
                options(),
                &transport,
                Some(&|e| {
                    events.lock().unwrap().push(byte_event(e));
                    Ok(())
                }),
            )
            .unwrap()
        } else {
            execute(
                &root,
                &store,
                &plan,
                options(),
                &transport,
                Some(&|e| {
                    events.lock().unwrap().push(public_event(e));
                    Ok(())
                }),
            )
            .unwrap()
        };
        assert_shared(&root_path, &cache_path, &f, body);
        let old = identity(&native(&root_path, &f.path.0));
        let cached = if raw {
            execute_bytes(
                &root,
                &store,
                std::slice::from_ref(&f),
                f.size,
                options(),
                &transport,
                None,
            )
            .unwrap()
        } else {
            execute(&root, &store, &plan, options(), &transport, None).unwrap()
        };
        assert_eq!(cached.files_cached, 1);
        assert_eq!(cached.files_downloaded, 0);
        assert_eq!(transport.calls.lock().unwrap().len(), 1);
        assert_eq!(old, identity(&native(&root_path, &f.path.0)));
        results.push((summary, events.into_inner().unwrap()));
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn byte_execution_raw_paths_and_retry_events() {
    let temp = tempfile::tempdir().unwrap();
    let paths = if cfg!(windows) {
        vec![
            b"mods/raw-\xed\xa0\x80.jar".to_vec(),
            "mods/猫.jar".as_bytes().to_vec(),
        ]
    } else {
        vec![
            b"mods/raw-\xed\xa0\x80.jar".to_vec(),
            "mods/猫.jar".as_bytes().to_vec(),
            b"mods/raw-\xff.jar".to_vec(),
        ]
    };
    for (index, path) in paths.into_iter().enumerate() {
        let root_path = temp.path().join(format!("root-{index}"));
        let cache_path = temp.path().join(format!("cache-{index}"));
        let root = ArtifactRoot::open(&root_path).unwrap();
        let store = Store::open(&cache_path).unwrap();
        // Existing parent exposes a native leaf-encoding rejection during the
        // read-only preflight, before the injected transport can be called.
        fs::create_dir(root_path.join("mods")).unwrap();
        let mut f = file(&path, b"opaque");
        f.fallback_url = Some("https://fixture.invalid/fallback".into());
        let transport = Fake::bodies(vec![
            Err(Error::named("SyntheticTransportFailure")),
            Ok(b"opaque".to_vec()),
        ]);
        let events = Mutex::new(vec![]);
        let result = execute_bytes(
            &root,
            &store,
            std::slice::from_ref(&f),
            f.size,
            Options {
                retries: 1,
                ..options()
            },
            &transport,
            Some(&|e| {
                events.lock().unwrap().push(byte_event(e));
                Ok(())
            }),
        );
        #[cfg(unix)]
        if result.as_ref().is_err_and(|e| e.name() == "BadPathName") {
            // APFS rejects invalid native UTF-8. Verify that this really is the
            // native filesystem capability, not an arbitrary executor error.
            fs::create_dir_all(root_path.join("probe")).unwrap();
            let probe = root_path.join("probe").join(
                crate::store::path_from_bytes(path.rsplit(|b| *b == b'/').next().unwrap()).unwrap(),
            );
            let error = fs::write(&probe, b"probe").unwrap_err();
            assert_eq!(
                error.raw_os_error(),
                Some(rustix::io::Errno::ILSEQ.raw_os_error())
            );
            assert_eq!(fs::read_dir(root_path.join("probe")).unwrap().count(), 0);
            assert!(transport.calls.lock().unwrap().is_empty());
            eprintln!(
                "raw byte execution unavailable: filesystem EILSEQ; Unicode and event representation controls remain required"
            );
            continue;
        }
        assert_eq!(result.unwrap().files_downloaded, 1);
        assert_shared(&root_path, &cache_path, &f, b"opaque");
        assert_eq!(
            *transport.calls.lock().unwrap(),
            vec![
                "https://fixture.invalid/opaque",
                "https://fixture.invalid/fallback"
            ]
        );
        let rows = events.into_inner().unwrap();
        let retry = rows.iter().find(|v| v.get("retry").is_some()).unwrap();
        assert_eq!(
            retry["retry"]["path"],
            serde_json::to_value(&f.path).unwrap()
        );
        assert_eq!(retry["retry"]["error"], "SyntheticTransportFailure");
        assert_eq!(retry["retry"]["attempt"], 1);
        assert_eq!(retry["retry"]["max_attempts"], 2);
        let done = rows.iter().find(|v| v.get("file").is_some()).unwrap();
        assert_eq!(done["file"]["path"], serde_json::to_value(&f.path).unwrap());
        assert_eq!(done["file"]["placement"], "hard_link");
    }
    // Raw event encoding is always testable, even on a filesystem rejecting it.
    assert_eq!(
        serde_json::to_value(BytePath(vec![255, 0])).unwrap(),
        json!([255, 0])
    );
}
#[test]
fn byte_execution_hash_failure_preserves_peer() {
    let temp = tempfile::tempdir().unwrap();
    let root_path = temp.path().join("root");
    let cache_path = temp.path().join("cache");
    let root = ArtifactRoot::open(&root_path).unwrap();
    let store = Store::open(&cache_path).unwrap();
    let old = file(b"mods/existing.jar", b"old bytes");
    execute_bytes(
        &root,
        &store,
        std::slice::from_ref(&old),
        old.size,
        options(),
        &Fake::bodies(vec![Ok(b"old bytes".to_vec())]),
        None,
    )
    .unwrap();
    let peer = temp.path().join("peer");
    fs::hard_link(native(&root_path, &old.path.0), &peer).unwrap();
    let before = identity(&peer);
    let next = file(&old.path.0, b"new bytes");
    let transport = Fake::bodies(vec![Ok(b"bad bytes".to_vec())]);
    assert_eq!(
        execute_bytes(
            &root,
            &store,
            std::slice::from_ref(&next),
            next.size,
            options(),
            &transport,
            None
        )
        .unwrap_err()
        .name(),
        "DownloadHashMismatch"
    );
    assert_shared(&root_path, &cache_path, &old, b"old bytes");
    assert_eq!(identity(&peer), before);
    assert_eq!(fs::read(&peer).unwrap(), b"old bytes");
    assert!(fs::metadata(&peer).unwrap().permissions().readonly());
    assert!(
        !cache_path
            .join(crate::store::object_path(&next.sha1).unwrap())
            .try_exists()
            .unwrap()
    );
}
#[test]
fn byte_execution_preflight_and_writer_failure() {
    let temp = tempfile::tempdir().unwrap();
    let root_path = temp.path().join("root");
    let root = ArtifactRoot::open(&root_path).unwrap();
    let store = Store::open(temp.path().join("cache")).unwrap();
    let transport = Fake::bodies(vec![]);
    for path in [
        b"../escape".as_slice(),
        b"/absolute",
        b"mods//a",
        b"mods/NUL.jar",
        b"mods/a. ",
        b"mods/a\x00",
        b"mods/a\\b",
    ] {
        let f = file(path, b"x");
        assert_eq!(
            execute_bytes(&root, &store, &[f], 1, options(), &transport, None)
                .unwrap_err()
                .name(),
            "UnsafePath"
        );
    }
    let f = file(b"mods/a.jar", b"opaque");
    for (invalid, expected) in [
        (
            Options {
                workers: 0,
                ..options()
            },
            "InvalidWorkerCount",
        ),
        (
            Options {
                retries: 6,
                ..options()
            },
            "InvalidRetryCount",
        ),
    ] {
        assert_eq!(
            execute_bytes(
                &root,
                &store,
                std::slice::from_ref(&f),
                f.size,
                invalid,
                &transport,
                None
            )
            .unwrap_err()
            .name(),
            expected
        );
    }
    assert_eq!(
        execute_bytes(
            &root,
            &store,
            &[f.clone(), f.clone()],
            2 * f.size,
            options(),
            &transport,
            None
        )
        .unwrap_err()
        .name(),
        "ConflictingDestination"
    );
    assert_eq!(
        execute_bytes(
            &root,
            &store,
            std::slice::from_ref(&f),
            f.size + 1,
            options(),
            &transport,
            None
        )
        .unwrap_err()
        .name(),
        "InvalidPlanTotal"
    );
    assert!(transport.calls.lock().unwrap().is_empty());
    assert_eq!(fs::read_dir(&root_path).unwrap().count(), 0);
    let transport = Fake::bodies(vec![Ok(b"opaque".to_vec())]);
    assert_eq!(
        execute_bytes(
            &root,
            &store,
            std::slice::from_ref(&f),
            f.size,
            options(),
            &transport,
            Some(&|_| Err(Error::named("WriteFailed")))
        )
        .unwrap_err()
        .name(),
        "WriteFailed"
    );
    // A small file has no intermediate progress event: source ordering emits
    // File only after verified materialization, so late writer failure retains
    // the immutable file. Do not invent a rollback guarantee.
    assert_eq!(fs::read(native(&root_path, &f.path.0)).unwrap(), b"opaque");
    assert!(
        fs::metadata(native(&root_path, &f.path.0))
            .unwrap()
            .permissions()
            .readonly()
    );
    let early_path = temp.path().join("early");
    let early = ArtifactRoot::open(&early_path).unwrap();
    let early_cache = temp.path().join("early-cache");
    let early_store = Store::open(&early_cache).unwrap();
    let body = vec![b'x'; PROGRESS_BYTE_INTERVAL as usize + 1];
    let large = file(b"mods/large.jar", &body);
    let transport = Fake::bodies(vec![Ok(body)]);
    assert_eq!(
        execute_bytes(
            &early,
            &early_store,
            std::slice::from_ref(&large),
            large.size,
            options(),
            &transport,
            Some(&|_| Err(Error::named("WriteFailed")))
        )
        .unwrap_err()
        .name(),
        "WriteFailed"
    );
    assert!(!native(&early_path, &large.path.0).try_exists().unwrap());
    assert!(
        !early_cache
            .join(crate::store::object_path(&large.sha1).unwrap())
            .try_exists()
            .unwrap()
    );
}
#[test]
fn byte_verify_is_noncreating_and_nofollow() {
    let temp = tempfile::tempdir().unwrap();
    let root_path = temp.path().join("root");
    let root = ArtifactRoot::open(&root_path).unwrap();
    let hash = format!("{:x}", Sha1::digest(b"opaque"));
    assert!(
        !root
            .verify_bytes(b"missing/nested/a.jar", &hash, 6)
            .unwrap()
    );
    assert!(!root_path.join("missing").try_exists().unwrap());
    for path in [
        b"../escape".as_slice(),
        b"/absolute",
        b"a//b",
        b"a\\b",
        b"a\0",
    ] {
        assert!(root.verify_bytes(path, &hash, 6).is_err());
    }
    #[cfg(unix)]
    {
        let outside = temp.path().join("owned-peer-dir");
        fs::create_dir(&outside).unwrap();
        let file = outside.join("opaque");
        fs::write(&file, b"opaque").unwrap();
        let before = identity(&file);
        std::os::unix::fs::symlink(&outside, root_path.join("alias")).unwrap();
        std::os::unix::fs::symlink(&file, root_path.join("leaf")).unwrap();
        assert!(root.verify_bytes(b"alias/opaque", &hash, 6).is_err());
        assert!(root.verify_bytes(b"leaf", &hash, 6).is_err());
        assert_eq!(identity(&file), before);
        assert_eq!(fs::read(&file).unwrap(), b"opaque");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    }
}

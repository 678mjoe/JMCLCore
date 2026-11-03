//! Native byte argv bridge for vanilla execute. No new runtime acquisition path.
use super::*;
fn options(args: &[std::ffi::OsString]) -> Option<Result<Request, ()>> {
    let Some(id) = args.first().filter(|v| !v.is_empty()) else {
        return Some(Err(()));
    };
    let mut p = Request {
        id: crate::native_bytes::os_bytes(id),
        directory: b".minecraft".to_vec(),
        store: b".jmclcore".to_vec(),
        source: Source::Official,
        os: None,
        arch: None,
        workers: 8,
        retries: 2,
    };
    let mut loader = false;
    for pair in args[1..].chunks(2) {
        let key = pair[0].to_str();
        let Some(value) = pair.get(1).filter(|v| !v.is_empty()) else {
            return Some(Err(()));
        };
        match key {
            Some("--fabric-loader" | "--neoforge-version" | "--forge-version") => loader = true,
            Some("--directory") => p.directory = crate::native_bytes::os_bytes(value),
            Some("--store-directory") => p.store = crate::native_bytes::os_bytes(value),
            Some("--source") => match value.to_str().and_then(Source::parse) {
                Some(v) => p.source = v,
                None => return Some(Err(())),
            },
            Some("--os") => match value.to_str() {
                Some(v @ ("linux" | "windows" | "osx")) => p.os = Some(v.into()),
                _ => return Some(Err(())),
            },
            Some("--arch") => {
                p.arch = Some(
                    match value.to_str() {
                        Some("amd64") => "x86_64",
                        Some("aarch64") => "arm64",
                        Some(v @ ("x86" | "x86_64" | "arm" | "arm64")) => v,
                        _ => return Some(Err(())),
                    }
                    .into(),
                )
            }
            Some("--workers") => match value
                .to_str()
                .and_then(|v| super::super::cli::number(v).ok())
            {
                Some(n @ 1..=32) => p.workers = n,
                _ => return Some(Err(())),
            },
            Some("--retries") => match value
                .to_str()
                .and_then(|v| super::super::cli::number(v).ok())
            {
                Some(n @ 0..=5) => p.retries = n,
                _ => return Some(Err(())),
            },
            // Validated by the original CLI even though vanilla has no processors.
            Some("--java-policy")
                if matches!(value.to_str(), Some("auto" | "local" | "managed")) => {}
            _ => return Some(Err(())),
        }
    }
    if loader { None } else { Some(Ok(p)) }
}
fn dispatch<W: Write, E: Write>(
    args: &[std::ffi::OsString],
    out: &mut W,
    err: &mut E,
    run: impl FnOnce(Request, &mut W, &mut E) -> io::Result<u8>,
) -> Option<io::Result<u8>> {
    if args.first().and_then(|v| v.to_str()) != Some("install")
        || args.get(1).and_then(|v| v.to_str()) != Some("execute")
        || !args.iter().any(|v| v.to_str().is_none())
    {
        return None;
    }
    match options(&args[2..])? {
        Ok(p) => Some(run(p, out, err)),
        Err(()) => Some(writeln!(err, "{}", super::super::cli::usage("execute")).map(|_| 2)),
    }
}
pub fn cli_os(
    args: &[std::ffi::OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<io::Result<u8>> {
    dispatch(args, out, err, |p, out, err| {
        let metadata = match version::HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(
                    err,
                    "failed to create install plan from {}: {}",
                    p.source.display(),
                    e.name()
                )?;
                return Ok(1);
            }
        };
        let files = match HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(err, "download failed: {}", e.name())?;
                return Ok(1);
            }
        };
        run_cli(
            p,
            &version::Service::new(metadata, ".jmclcore/metadata-cache"),
            &files,
            out,
            err,
        )
    })
}
#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    fn raw() -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![255])
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xd800])
        }
    }
    #[test]
    fn byte_id_roots_aliases_and_numeric_options_are_preserved() {
        let native = raw();
        let expected = crate::native_bytes::os_bytes(&native);
        let p = options(&[
            native.clone(),
            "--directory".into(),
            native.clone(),
            "--store-directory".into(),
            native,
            "--arch".into(),
            "aarch64".into(),
            "--workers".into(),
            "+3_2".into(),
            "--retries".into(),
            "0".into(),
            "--java-policy".into(),
            "managed".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.id, expected);
        assert_eq!(p.directory, expected);
        assert_eq!(p.store, expected);
        assert_eq!(p.arch.as_deref(), Some("arm64"));
        assert_eq!((p.workers, p.retries), (32, 0));
    }
    #[test]
    fn later_valid_options_replace_prior_values() {
        let p = options(&[
            raw(),
            "--directory".into(),
            "first".into(),
            "--directory".into(),
            raw(),
            "--workers".into(),
            "1".into(),
            "--workers".into(),
            "2".into(),
            "--retries".into(),
            "5".into(),
            "--retries".into(),
            "-0".into(),
            "--arch".into(),
            "x86".into(),
            "--arch".into(),
            "amd64".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.directory, crate::native_bytes::os_bytes(&raw()));
        assert_eq!((p.workers, p.retries), (2, 0));
        assert_eq!(p.arch.as_deref(), Some("x86_64"));
    }
    #[test]
    fn invalid_options_fail_before_metadata_or_files() {
        for tail in [
            vec!["--directory".into()],
            vec!["--store-directory".into(), "".into()],
            vec!["--workers".into(), "0".into()],
            vec!["--workers".into(), "33".into()],
            vec!["--retries".into(), "6".into()],
            vec!["--java-policy".into(), raw()],
            vec!["--fabric-loader".into()],
            vec![
                "--neoforge-version".into(),
                "v".into(),
                "--os".into(),
                "bad".into(),
            ],
        ] {
            let mut args = vec!["install".into(), "execute".into(), raw()];
            args.extend(tail);
            let mut out = vec![];
            let mut err = vec![];
            assert_eq!(
                dispatch(&args, &mut out, &mut err, |_, _, _| panic!("must not plan"))
                    .unwrap()
                    .unwrap(),
                2
            );
            assert!(out.is_empty());
            assert!(err.starts_with(b"usage: jmcl-core install execute"));
        }
    }
    #[test]
    fn unrelated_utf8_and_valid_loaders_remain_unclaimed() {
        for args in [
            vec!["install".into(), "execute".into(), "v".into()],
            vec!["install".into(), "prepare".into(), raw()],
            vec![
                "install".into(),
                "execute".into(),
                raw(),
                "--forge-version".into(),
                "v".into(),
            ],
        ] {
            assert!(dispatch(&args, &mut vec![], &mut vec![], |_, _, _| panic!()).is_none());
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod execution_tests {
    use super::*;
    use serde_json::json;
    use sha1::{Digest, Sha1};
    use std::{
        cell::RefCell,
        collections::VecDeque,
        ffi::OsString,
        fs,
        os::unix::{ffi::OsStringExt, fs::MetadataExt},
        path::Path,
        sync::atomic::{AtomicUsize, Ordering},
    };
    const DATA: &[u8] = b"inert synthetic client content";
    const META: &str = "https://fixture.invalid/metadata";
    const FILE: &str = "https://fixture.invalid/client";
    struct Metadata(RefCell<VecDeque<(String, Vec<u8>)>>);
    impl version::Transport for Metadata {
        fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, crate::metadata::Error> {
            let (expected, body) = self
                .0
                .borrow_mut()
                .pop_front()
                .expect("unexpected metadata request");
            assert_eq!(url, expected);
            Ok(body)
        }
    }
    struct Files {
        calls: AtomicUsize,
        fail: bool,
    }
    impl Transport for Files {
        fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, Error> {
            assert_eq!(url, FILE);
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Ok(500);
            }
            sink.write_all(DATA)?;
            Ok(200)
        }
    }
    fn hash(b: &[u8]) -> String {
        format!("{:x}", Sha1::digest(b))
    }
    fn service(owner: &Path) -> version::Service<Metadata> {
        let body=serde_json::to_vec(&json!({"id":[255],"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":FILE,"sha1":hash(DATA),"size":DATA.len()}}})).unwrap();
        let manifest=serde_json::to_vec(&json!({"latest":{"release":"v","snapshot":"v"},"versions":[{"id":[255],"type":"release","url":META,"sha1":hash(&body),"time":"t","releaseTime":"t"}]})).unwrap();
        let mut svc = version::Service::new(
            Metadata(RefCell::new(VecDeque::from([
                (Source::Official.manifest_url().into(), manifest),
                (META.into(), body),
            ]))),
            owner.join("metadata-cache"),
        );
        svc.now = 1000;
        svc
    }
    fn arguments(root: &Path, store: &Path) -> Vec<OsString> {
        vec![
            "install".into(),
            "execute".into(),
            OsString::from_vec(vec![255]),
            "--directory".into(),
            root.as_os_str().into(),
            "--store-directory".into(),
            store.as_os_str().into(),
            "--workers".into(),
            "1".into(),
            "--retries".into(),
            "0".into(),
            "--java-policy".into(),
            "managed".into(),
        ]
    }
    #[test]
    fn native_dispatch_writes_byte_receipt_and_preserves_hardlink_storage() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join(OsString::from_vec(b"game-\xff".to_vec()));
        let store = owner
            .path()
            .join(OsString::from_vec(b"store-\xfe".to_vec()));
        let svc = service(owner.path());
        let files = Files {
            calls: AtomicUsize::new(0),
            fail: false,
        };
        let mut out = vec![];
        let mut err = vec![];
        let args = arguments(&root, &store);
        let code = dispatch(&args, &mut out, &mut err, |p, o, e| {
            run_cli(p, &svc, &files, o, e)
        })
        .unwrap()
        .unwrap();
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        let mut expected =
            b"version\t\xff\ndownloaded\t1\ncached\t0\nhardLinked\t1\ncopied\t0\n".to_vec();
        expected.extend_from_slice(
            format!(
                "verifiedBytes\t{}\ntransferredBytes\t{}\n",
                DATA.len(),
                DATA.len()
            )
            .as_bytes(),
        );
        assert_eq!(out, expected);
        assert_eq!(
            err,
            format!(
                "download progress: 1/1 files, {}/{} bytes verified\n",
                DATA.len(),
                DATA.len()
            )
            .as_bytes()
        );
        assert_eq!(files.calls.load(Ordering::SeqCst), 1);
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(".jmcl/install.json")).unwrap()).unwrap();
        assert_eq!(receipt["version_id"], json!([255]));
        let object = store.join(crate::store::object_path(&hash(DATA)).unwrap());
        let m = fs::metadata(&object).unwrap();
        assert_eq!(m.nlink(), 2);
        assert!(m.permissions().readonly());
        assert_eq!(fs::read(&object).unwrap(), DATA);
        out.clear();
        err.clear();
        assert_eq!(
            dispatch(&args, &mut out, &mut err, |p, o, e| run_cli(
                p, &svc, &files, o, e
            ))
            .unwrap()
            .unwrap(),
            0
        );
        assert_eq!(files.calls.load(Ordering::SeqCst), 1);
        let mut expected =
            b"version\t\xff\ndownloaded\t0\ncached\t1\nhardLinked\t1\ncopied\t0\n".to_vec();
        expected.extend_from_slice(
            format!("verifiedBytes\t{}\ntransferredBytes\t0\n", DATA.len()).as_bytes(),
        );
        assert_eq!(out, expected);
        assert_eq!(
            err,
            format!(
                "download progress: 1/1 files, {}/{} bytes verified\n",
                DATA.len(),
                DATA.len()
            )
            .as_bytes()
        );
        assert_eq!(fs::metadata(&object).unwrap().ino(), m.ino());
    }
    #[test]
    fn native_dispatch_blocked_root_does_not_fetch_files() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("file");
        let store = owner.path().join("store");
        fs::write(&root, b"canary").unwrap();
        let svc = service(owner.path());
        let files = Files {
            calls: AtomicUsize::new(0),
            fail: false,
        };
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            dispatch(&arguments(&root, &store), &mut out, &mut err, |p, o, e| {
                run_cli(p, &svc, &files, o, e)
            })
            .unwrap()
            .unwrap(),
            1
        );
        assert_eq!(files.calls.load(Ordering::SeqCst), 0);
        assert!(out.is_empty());
        assert!(err.starts_with(b"failed to open install root: "));
        assert_eq!(fs::read(root).unwrap(), b"canary");
        assert!(!store.exists());
    }
    #[test]
    fn native_dispatch_failed_transfer_preserves_previous_receipt() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("game");
        let store = owner.path().join("store");
        fs::create_dir_all(root.join(".jmcl")).unwrap();
        let receipt = root.join(".jmcl/install.json");
        fs::write(&receipt, b"previous receipt").unwrap();
        let svc = service(owner.path());
        let files = Files {
            calls: AtomicUsize::new(0),
            fail: true,
        };
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            dispatch(&arguments(&root, &store), &mut out, &mut err, |p, o, e| {
                run_cli(p, &svc, &files, o, e)
            })
            .unwrap()
            .unwrap(),
            1
        );
        assert_eq!(files.calls.load(Ordering::SeqCst), 1);
        assert!(out.is_empty());
        assert!(err.starts_with(b"download failed: "));
        assert_eq!(fs::read(receipt).unwrap(), b"previous receipt");
    }
}

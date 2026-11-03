//! Native argv bridge for vanilla prepare; loader arguments keep the existing route.
use super::{prepare::Request, *};
fn options(args: &[std::ffi::OsString]) -> Option<Result<Request, ()>> {
    let Some(id) = args.first().filter(|v| !v.is_empty()) else {
        return Some(Err(()));
    };
    let mut p = Request {
        id: crate::native_bytes::os_bytes(id),
        directory: b".minecraft".to_vec(),
        source: Source::Official,
        os: None,
        arch: None,
    };
    let mut loader = false;
    for pair in args[1..].chunks(2) {
        let key = pair[0].to_str();
        let Some(value) = pair.get(1).filter(|v| !v.is_empty()) else {
            return Some(Err(()));
        };
        match key {
            Some("--fabric-loader" | "--neoforge-version") => loader = true,
            // The pinned prepare CLI parses Forge but omits it from selection.
            Some("--forge-version") => {}
            Some("--directory") => p.directory = crate::native_bytes::os_bytes(value),
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
        || args.get(1).and_then(|v| v.to_str()) != Some("prepare")
        || !args.iter().any(|v| v.to_str().is_none())
    {
        return None;
    }
    match options(&args[2..])? {
        Ok(p) => Some(run(p, out, err)),
        Err(()) => Some(writeln!(err, "{}", super::super::cli::usage("prepare")).map(|_| 2)),
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
        run(
            p,
            &version::Service::new(metadata, ".jmclcore/metadata-cache"),
            out,
            err,
        )
    })
}
fn run<M: version::Transport>(
    p: Request,
    svc: &version::Service<M>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let mut target = match Target::host() {
        Ok(t) => t,
        Err(e) => {
            writeln!(err, "unsupported host platform: {}", e.name())?;
            return Ok(1);
        }
    };
    if let Some(os) = p.os {
        target.os = os;
    }
    if let Some(arch) = p.arch {
        target.arch = arch;
    }
    let plan = match svc.vanilla_read_plan(&p.id, p.source, target) {
        Ok(p) => p,
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
    let root = match crate::store::path_from_bytes(&p.directory)
        .and_then(crate::store::ArtifactRoot::open)
    {
        Ok(r) => r,
        Err(e) => {
            writeln!(
                err,
                "failed to open install root: {}",
                Error::from(e).name()
            )?;
            return Ok(1);
        }
    };
    let prepared = match plan.prepare_game_native(&root) {
        Ok(p) => p,
        Err(e) => {
            writeln!(err, "runtime preparation failed: {}", e.name())?;
            return Ok(1);
        }
    };
    if p.source == Source::Bmclapi {
        writeln!(out, "# source\t{}", p.source.display())?;
    }
    out.write_all(b"version\t")?;
    out.write_all(&p.id)?;
    out.write_all(b"\n")?;
    if let Some(path) = prepared.native_directory {
        writeln!(out, "nativeDirectory\t{path}")?;
    }
    writeln!(
        out,
        "nativeFiles\t{}\nprojectionsCreated\t{}\nprojectionsCached\t{}",
        prepared.native_files, prepared.projections_created, prepared.projections_cached
    )?;
    Ok(0)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    fn raw() -> OsString {
        OsString::from_vec(vec![255])
    }
    #[test]
    fn byte_identity_directory_and_aliases_are_preserved() {
        let p = options(&[
            raw(),
            "--directory".into(),
            raw(),
            "--arch".into(),
            "aarch64".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(p.id, [255]);
        assert_eq!(p.directory, [255]);
        assert_eq!(p.arch.as_deref(), Some("arm64"));
    }
    #[test]
    fn invalid_options_stop_before_transport() {
        for tail in [
            vec!["--directory".into()],
            vec!["--fabric-loader".into()],
            vec!["--forge-version".into(), "".into()],
            vec![
                "--neoforge-version".into(),
                "v".into(),
                "--arch".into(),
                "bad".into(),
            ],
            vec!["--arch".into(), raw()],
            vec!["--workers".into(), "1".into()],
        ] {
            let mut args = vec!["install".into(), "prepare".into(), raw()];
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
            assert!(err.starts_with(b"usage: jmcl-core install prepare"));
        }
    }
    #[test]
    fn utf8_and_loader_requests_remain_unclaimed() {
        assert!(
            dispatch(
                &["install".into(), "prepare".into(), "v".into()],
                &mut vec![],
                &mut vec![],
                |_, _, _| panic!()
            )
            .is_none()
        );
        assert!(options(&[raw(), "--fabric-loader".into(), "v".into()]).is_none());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod execution_tests {
    use super::super::prepare::execution_tests::{request, service};
    use super::*;
    #[test]
    fn synthetic_cli_writes_raw_identity_and_exact_summary() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("game");
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            run(request(&root), &service(owner.path()), &mut out, &mut err).unwrap(),
            0
        );
        assert!(err.is_empty());
        assert!(out.starts_with(b"version\t\xff\nnativeDirectory\t"));
        assert!(out.ends_with(b"\nnativeFiles\t0\nprojectionsCreated\t0\nprojectionsCached\t0\n"));
    }
    #[test]
    fn synthetic_cli_blocked_root_preserves_original_file() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("file");
        std::fs::write(&root, b"canary").unwrap();
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            run(request(&root), &service(owner.path()), &mut out, &mut err).unwrap(),
            1
        );
        assert!(out.is_empty());
        assert!(err.starts_with(b"failed to open install root: "));
        assert_eq!(std::fs::read(&root).unwrap(), b"canary");
    }
}

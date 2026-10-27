//! Narrow native argv bridge for the existing vanilla planning read path.
use super::*;

#[cfg(any(unix, windows))]
pub(super) fn parse_options(args: &[std::ffi::OsString]) -> Option<Result<(Request, bool), ()>> {
    let Some(id) = args.first().filter(|s| !s.is_empty()) else {
        return Some(Err(()));
    };
    let mut request = Request {
        id: crate::native_bytes::os_bytes(id),
        source: Source::Official,
        os: None,
        arch: None,
    };
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        let key = args[i].to_str();
        if key == Some("--json") {
            json = true;
            i += 1;
            continue;
        }
        // Presence claims the existing loader path, even if missing/empty/malformed.
        // Earlier invalid arguments have already produced their source-defined error.
        if matches!(
            key,
            Some("--fabric-loader" | "--neoforge-version" | "--forge-version")
        ) {
            return None;
        }
        let Some(value) = args.get(i + 1).and_then(|s| s.to_str()) else {
            return Some(Err(()));
        };
        match key {
            Some("--source") => match Source::parse(value) {
                Some(source) => request.source = source,
                None => return Some(Err(())),
            },
            Some("--os") if matches!(value, "windows" | "linux" | "osx") => {
                request.os = Some(value.into())
            }
            Some("--arch") => {
                request.arch = Some(
                    match value {
                        "amd64" => "x86_64",
                        "aarch64" => "arm64",
                        "x86" | "x86_64" | "arm" | "arm64" => value,
                        _ => return Some(Err(())),
                    }
                    .into(),
                )
            }
            _ => return Some(Err(())),
        }
        i += 2;
    }
    Some(Ok((request, json)))
}
#[cfg(any(unix, windows))]
fn selected(args: &[std::ffi::OsString]) -> bool {
    args.first().and_then(|s| s.to_str()) == Some("install")
        && args.get(1).and_then(|s| s.to_str()) == Some("plan")
        && args.iter().any(|s| s.to_str().is_none())
}
// Production and tests share classification/parse/error dispatch. The injected
// continuation is never called for rejected or unclaimed arguments.
pub(super) fn dispatch<W: Write, E: Write>(
    args: &[std::ffi::OsString],
    out: &mut W,
    err: &mut E,
    run: impl FnOnce(Request, bool, &mut W, &mut E) -> std::io::Result<u8>,
) -> Option<std::io::Result<u8>> {
    #[cfg(any(unix, windows))]
    {
        if !selected(args) {
            return None;
        }
        match parse_options(&args[2..])? {
            Ok((request, json)) => Some(run(request, json, out, err)),
            Err(()) => Some(writeln!(err, "{}", cli_usage("install.plan")).map(|_| 2)),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (args, out, err, run);
        None
    }
}
/// Ordinary UTF-8 arguments and all encountered loader options retain old routing.
pub fn cli_os(
    args: &[std::ffi::OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<std::io::Result<u8>> {
    dispatch(args, out, err, |request, json, out, err| {
        let transport = match HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(
                    err,
                    "failed to create install plan from {}: {}",
                    request.source.display(),
                    e.name()
                )?;
                return Ok(1);
            }
        };
        execute(
            &Service::new(transport, ".jmclcore/metadata-cache"),
            &request,
            json,
            out,
            err,
        )
    })
}

#[cfg(all(test, any(unix, windows)))]
#[path = "version_plan_native_portable_tests.rs"]
mod portable_tests;

//! Pure native parser/dispatcher tests; never construct a service or transport.
use super::*;
use std::{cell::Cell, ffi::OsString};
fn opaque() -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![255])
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xdc00])
    }
}
fn args(tail: &[&str]) -> Vec<OsString> {
    let mut a = vec!["install".into(), "plan".into(), opaque()];
    a.extend(tail.iter().map(OsString::from));
    a
}
#[test]
fn native_plan_dispatch_preserves_bytes_options_and_repetition() {
    let calls = Cell::new(0);
    let mut out = vec![];
    let mut err = vec![];
    let result = dispatch(
        &args(&[
            "--source", "bmclapi", "--source", "official", "--os", "linux", "--os", "windows",
            "--arch", "amd64", "--arch", "aarch64", "--json", "--json",
        ]),
        &mut out,
        &mut err,
        |p, json, out, _| {
            calls.set(calls.get() + 1);
            assert_eq!(p.id, crate::native_bytes::os_bytes(&opaque()));
            assert_eq!(p.source, Source::Official);
            assert_eq!(p.os.as_deref(), Some("windows"));
            assert_eq!(p.arch.as_deref(), Some("arm64"));
            assert!(json);
            out.write_all(&p.id)?;
            Ok(0)
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, 0);
    assert_eq!(calls.get(), 1);
    assert_eq!(out, crate::native_bytes::os_bytes(&opaque()));
    assert!(err.is_empty());
}
#[test]
fn native_plan_left_to_right_errors_and_loader_exclusions_never_continue() {
    let mut cases = vec![
        (vec!["install".into(), "plan".into(), "版本😀".into()], None),
        (vec!["install".into(), "execute".into(), opaque()], None),
        (vec!["version".into(), "show".into(), opaque()], None),
        (args(&["--source"]), Some(2)),
        (args(&["--os", "bad"]), Some(2)),
        (args(&["--arch", "bad"]), Some(2)),
        (args(&["--unknown", "x", "--forge-version"]), Some(2)),
        (args(&["--source", "bad", "--fabric-loader", "x"]), Some(2)),
        (args(&["--forge-version", "x", "--unknown"]), None),
        (args(&["--fabric-loader"]), None),
        (args(&["--neoforge-version", ""]), None),
        (
            args(&["--fabric-loader", "a", "--forge-version", "b"]),
            None,
        ),
    ];
    let mut nontext = args(&["--source"]);
    nontext.push(opaque());
    cases.push((nontext, Some(2)));
    for (a, expected) in cases {
        let calls = Cell::new(0);
        let mut out = vec![];
        let mut err = vec![];
        let result = dispatch(&a, &mut out, &mut err, |_, _, _, _| {
            calls.set(calls.get() + 1);
            Ok(99)
        });
        assert_eq!(result.map(Result::unwrap), expected);
        assert_eq!(calls.get(), 0);
        assert!(out.is_empty());
        assert_eq!(err.is_empty(), expected.is_none());
    }
}

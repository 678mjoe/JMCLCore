//! Pure dispatch fixtures, portable across supported Unix/Windows targets.
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
        OsString::from_wide(&[0xd800])
    }
}
fn args(tail: &[&str]) -> Vec<OsString> {
    let mut a = vec!["version".into(), "show".into(), opaque()];
    a.extend(tail.iter().map(OsString::from));
    a
}
#[test]
fn native_show_dispatch_preserves_id_and_last_source_without_io() {
    let calls = Cell::new(0);
    let mut out = vec![];
    let mut err = vec![];
    let code = dispatch(
        &args(&[
            "--source", "bmclapi", "--json", "--source", "official", "--json",
        ]),
        &mut out,
        &mut err,
        |p, json, out, _| {
            calls.set(calls.get() + 1);
            assert_eq!(p.id, crate::native_bytes::os_bytes(&opaque()));
            assert_eq!(p.source, Source::Official);
            assert!(json);
            out.write_all(&p.id)?;
            Ok(0)
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(code, 0);
    assert_eq!(calls.get(), 1);
    assert_eq!(out, crate::native_bytes::os_bytes(&opaque()));
    assert!(err.is_empty());
}
#[test]
fn native_show_unclaimed_unicode_routes_and_invalid_options_never_continue() {
    let mut cases: Vec<(Vec<OsString>, Option<u8>)> = vec![
        (vec!["version".into(), "show".into(), "版本😀".into()], None),
        (vec!["version".into(), "resolve".into(), opaque()], None),
        (vec!["install".into(), "execute".into(), opaque()], None),
        (args(&["--source"]), Some(2)),
        (args(&["--source", "bad"]), Some(2)),
        (args(&["--unknown"]), Some(2)),
        (args(&["--json", "--unknown", "x"]), Some(2)),
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

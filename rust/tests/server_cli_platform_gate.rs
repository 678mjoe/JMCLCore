//! This is a platform-rejection test, not native server support.
#[cfg(all(not(target_os = "linux"), unix))]
fn raw() -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::OsString::from_vec(b"raw-\xff".to_vec())
}
#[cfg(windows)]
fn raw() -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    std::ffi::OsString::from_wide(&[0xD800])
}
#[cfg(not(target_os = "linux"))]
#[test]
fn unsupported_local_server_routes_precede_native_argument_parsing() {
    use jmcl_core_rust::content::local_cli_os;
    let t = tempfile::tempdir().unwrap();
    for family in ["mods", "datapacks"] {
        for op in ["list", "enable", "disable", "remove"] {
            for tail in [
                vec!["demo".into(), "--servers-directory".into(), raw()],
                vec![
                    raw(),
                    "--servers-directory".into(),
                    t.path().join("服务器").into_os_string(),
                ],
                vec!["demo".into(), "--project".into(), raw()],
                vec!["demo".into(), "--unknown".into(), raw()],
                vec![
                    "demo".into(),
                    "--servers-directory".into(),
                    t.path().join("服务器").into_os_string(),
                ],
            ] {
                let mut args = vec!["server".into(), family.into(), op.into()];
                args.extend(tail);
                let (mut out, mut err) = (vec![], vec![]);
                assert_eq!(local_cli_os(&args, &mut out, &mut err).unwrap().unwrap(), 1);
                assert!(out.is_empty());
                assert_eq!(
                    err,
                    b"Server management is currently supported only on Linux\n"
                );
                assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
            }
        }
    }
}

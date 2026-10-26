#[cfg(target_os = "linux")]
#[test]
fn invalid_native_server_id_precedes_uninterpreted_tail() {
    use jmcl_core_rust::content::local_cli_os;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let t = tempfile::tempdir().unwrap();
    for family in ["mods", "datapacks"] {
        for op in ["list", "enable", "disable", "remove"] {
            for id in [
                b"\xff".as_slice(),
                b"demo\xff",
                b"\xed\xa0\x80",
                b"\xc0\xaf",
            ] {
                let mut args = vec![
                    "server".into(),
                    family.into(),
                    op.into(),
                    OsString::from_vec(id.to_vec()),
                    "--project".into(),
                    OsString::from_vec(b"\xff".to_vec()),
                ];
                let (mut out, mut err) = (vec![], vec![]);
                assert_eq!(
                    local_cli_os(&args, &mut out, &mut err)
                        .expect("recognized raw-ID rejection")
                        .unwrap(),
                    1
                );
                assert!(out.is_empty());
                assert_eq!(err, b"error: InvalidArguments\n");
                assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
                // Do not broaden the independent instance raw-ID route.
                args[0] = "instance".into();
                out.clear();
                err.clear();
                assert!(local_cli_os(&args, &mut out, &mut err).is_none());
                assert!(out.is_empty());
                assert!(err.is_empty());
            }
        }
    }
}

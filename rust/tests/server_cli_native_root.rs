#[cfg(target_os = "linux")]
#[test]
fn native_server_root_reaches_local_id_validation_without_filesystem_access() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let t = tempfile::tempdir().unwrap();
    let mut raw = t.path().as_os_str().as_encoded_bytes().to_vec();
    raw.extend_from_slice(b"/raw-\xff");
    for family in ["mods", "datapacks"] {
        for op in ["list", "enable", "disable", "remove"] {
            let args = vec![
                "server".into(),
                family.into(),
                op.into(),
                "../bad".into(),
                "--servers-directory".into(),
                OsString::from_vec(raw.clone()),
            ];
            let (mut out, mut err) = (vec![], vec![]);
            let result = jmcl_core_rust::content::local_cli_os(&args, &mut out, &mut err);
            assert_eq!(
                result.expect("recognized local native-root route").unwrap(),
                1
            );
            assert!(out.is_empty());
            assert_eq!(err, b"error: InvalidArguments\n");
            assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
        }
    }
}

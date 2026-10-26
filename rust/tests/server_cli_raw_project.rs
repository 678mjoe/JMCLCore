#[cfg(target_os = "linux")]
#[test]
fn native_project_retains_cli_order_and_invalid_text_id_precedes_tail() {
    use jmcl_core_rust::content::local_cli_os;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let t = tempfile::tempdir().unwrap();
    let blocked = t.path().join("blocked");
    std::fs::write(&blocked, b"preserve").unwrap();
    for family in ["mods", "datapacks"] {
        for op in ["list", "enable", "disable", "remove"] {
            for id in ["../bad", "服务器", "", "demo"] {
                let mut args = vec![
                    "server".into(),
                    family.into(),
                    op.into(),
                    id.into(),
                    "--servers-directory".into(),
                    blocked.clone().into_os_string(),
                    "--project".into(),
                    OsString::from_vec(vec![0xff]),
                ];
                let (mut out, mut err) = (vec![], vec![]);
                assert_eq!(
                    local_cli_os(&args, &mut out, &mut err)
                        .expect("native local project route")
                        .unwrap(),
                    1
                );
                assert!(out.is_empty());
                assert_eq!(
                    err,
                    if id != "demo" || op == "list" {
                        b"error: InvalidArguments\n".as_slice()
                    } else {
                        b"no server matches the given id\n"
                    }
                );
                if id != "demo" {
                    // ID validation must precede even a wholly uninterpreted raw tail.
                    args.push(OsString::from_vec(b"--\xff".to_vec()));
                    out.clear();
                    err.clear();
                    assert_eq!(local_cli_os(&args, &mut out, &mut err).unwrap().unwrap(), 1);
                    assert!(out.is_empty());
                    assert_eq!(err, b"error: InvalidArguments\n");
                }
                assert_eq!(std::fs::read(&blocked).unwrap(), b"preserve");
                assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 1);
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cli_local_display_preserves_stored_id_precedence() {
    use jmcl_core_rust::server_content;
    use serde_json::json;
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("demo");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        dir.join("server.json"),
        json!({"id":"bad id","name":[255],"version_id":"1.20.1"}).to_string(),
    )
    .unwrap();
    for family in ["mods", "datapacks"] {
        for op in ["list", "enable", "disable", "remove"] {
            let mut args: Vec<String> = [
                family,
                op,
                "demo",
                "--servers-directory",
                t.path().to_str().unwrap(),
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            if op != "list" {
                args.extend(["--file".into(), format!("{family}/opaque")]);
            }
            let (mut out, mut err) = (vec![], vec![]);
            let code = server_content::cli(&args, &mut out, &mut err)
                .unwrap()
                .unwrap();
            assert_eq!(code, 1);
            assert!(out.is_empty());
            assert_eq!(err, b"no server matches the given id\n");
            assert!(!dir.join(".jmcl").exists());
        }
    }
}

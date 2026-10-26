//! File-selector dynamic coercion only; empty owned registries remain unchanged.
use serde_json::json;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
fn pinned_oracle() -> String {
    let source = std::env::var("JMCL_ZIG_SOURCE").expect("pinned source checkout required");
    let head = Command::new("git")
        .args(["-C", &source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .args(["-C", &source, "diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    std::env::var("JMCL_ZIG_ORACLE").unwrap()
}

#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and JMCL_ZIG_ORACLE"]
fn dynamic_file_array_parse_and_selector_errors_match() {
    let zig = pinned_oracle();
    let mut cases: Vec<(&str, Option<&str>)> = [
        "\"mods/a.jar\"",
        "[109,111,100,115,47,97,46,106,97,114]",
        "[\"109\",111.0,100,115,47,97,46,106,97,114]",
        "[109.00000000000000001,111,100,115,47,97,46,106,97,114]",
        "[\"109.00000000000000001\",111,100,115,47,97,46,106,97,114]",
        "[255]",
        "[255.000000000000001]",
        "[\"255.000000000000001\"]",
        "[256]",
        "[-1]",
        "[-0]",
        "[1.5]",
        "[true]",
        "[null]",
        "[{}]",
        "[[1]]",
        "[]",
        "null",
        "[47]",
        "[46,46,47,97]",
        "[\"+1\"]",
        "[\"1_0\"]",
        "[\"0x1.b4p6\"]",
        "[1e400]",
        "[1e-400]",
    ]
    .into_iter()
    .map(|raw| (raw, None))
    .collect();
    cases.extend([
        ("[256]", Some("bad/ref")),
        ("[47]", Some("bad/ref")),
        ("[]", Some("bad/ref")),
        ("[255]", Some("project")),
        ("null", Some("project")),
        ("[1.5]", Some("project")),
    ]);
    for server in [false, true] {
        if server && !cfg!(target_os = "linux") {
            continue;
        }
        for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
            if server && !matches!(family, "mods" | "datapacks") {
                continue;
            }
            for op in ["enable", "disable", "remove"] {
                for &(raw, project) in &cases {
                    let mut outputs = vec![];
                    for exe in [zig.as_str(), env!("CARGO_BIN_EXE_jmcl-core-rust")] {
                        let t = tempfile::tempdir().unwrap();
                        let home = t.path().join("demo");
                        fs::create_dir(&home).unwrap();
                        let game = if server {
                            fs::write(
                                home.join("server.json"),
                                br#"{"id":"demo","name":"Demo","version_id":"demo"}"#,
                            )
                            .unwrap();
                            fs::write(home.join("server.properties"), b"level-name=World\n")
                                .unwrap();
                            home.clone()
                        } else {
                            home.join(".minecraft")
                        };
                        let content = if family == "datapacks" {
                            game.join(if server { "World" } else { "saves/World" })
                        } else {
                            game
                        };
                        fs::create_dir_all(content.join(".jmcl")).unwrap();
                        let registry = content.join(".jmcl/content.json");
                        fs::write(&registry, b"{\"schema_version\":1,\"entries\":[]}").unwrap();
                        let before = fs::read(&registry).unwrap();
                        let before_count = fs::read_dir(&content).unwrap().count();
                        let mut params = if server {
                            json!({"directory":".","id":"demo"})
                        } else {
                            json!({"directory":"demo/.minecraft","world":"World"})
                        };
                        if let Some(project) = project {
                            params["project"] = json!(project);
                        }
                        let mut params = params.to_string();
                        params.pop();
                        params.push_str(&format!(",\"file\":{raw}}}"));
                        let input = format!(
                            "{{\"protocol\":1,\"id\":\"selector\",\"method\":\"{}{family}.{op}\",\"params\":{params}}}\n",
                            if server { "server." } else { "" }
                        );
                        let mut c = Command::new(exe)
                            .arg("rpc")
                            .current_dir(t.path())
                            .stdin(Stdio::piped())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .spawn()
                            .unwrap();
                        c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
                        let out = c.wait_with_output().unwrap();
                        assert!(out.status.success());
                        assert!(out.stderr.is_empty());
                        assert_eq!(fs::read(&registry).unwrap(), before);
                        assert_eq!(fs::read_dir(&content).unwrap().count(), before_count);
                        outputs.push(out.stdout);
                    }
                    assert_eq!(
                        outputs[0], outputs[1],
                        "server={server} {family}.{op} file={raw} project={project:?}"
                    );
                }
            }
        }
    }
}

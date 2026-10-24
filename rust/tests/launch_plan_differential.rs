//! No real accounts or processes. All tokens below are synthetic fixture strings.
use jmcl_core_rust::{
    java::detect::{CandidateSource, Runtime},
    launch::{self, Auth, Input, Options},
    metadata::{self, Source},
    plan::{self, Target},
};
use serde_json::{Value, json};
fn runtime() -> Runtime {
    Runtime {
        executable: "/fake java/bin/java".into(),
        source: CandidateSource::Explicit,
        major_version: 17,
        specification_version: "17".into(),
        version: None,
        runtime_version: None,
        vendor: None,
        java_home: None,
        vm_name: None,
        os_arch: "amd64".into(),
        os_version: Some("6.8.0".into()),
        architecture: Some("x86_64".into()),
        data_model: Some(64),
    }
}
fn base() -> Value {
    json!({"id":"fixture","type":"release","mainClass":"fake.Main","libraries":[],"downloads":{"client":{"sha1":"1".repeat(40),"size":0,"url":"https://example.invalid/client"}},"javaVersion":{"component":"test","majorVersion":17},"arguments":{"game":["--token","${auth_access_token}","${auth_session}","${auth_player_name}","${resolution_width}"],"jvm":["-cp","${classpath}","-Dnative=${natives_directory}"],"default-user-jvm":["-Xmx1G"]}})
}
fn check(md: Value, opts: Value, force: bool, major: u16, arch: Option<&str>, oracle: bool) {
    let md = metadata::metadata(md.to_string().as_bytes()).unwrap();
    let mut install = plan::build(
        &md,
        None,
        Source::Official,
        Target {
            os: opts["__target_os"].as_str().unwrap_or("linux").into(),
            arch: "x86_64".into(),
            os_version: None,
        },
    )
    .unwrap();
    if opts["__virtual"].as_bool() == Some(true) {
        install.asset_projections.push(plan::AssetProjection {
            source_path: "assets/objects/fake".into(),
            destination_path: "assets/virtual/legacy/fake".into(),
            kind: "virtual_assets".into(),
        });
    }
    let mut rt = runtime();
    rt.major_version = major;
    rt.architecture = arch.map(Into::into);
    let jvm: Vec<&str> = opts["extra_jvm_args"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let game: Vec<&str> = opts["extra_game_args"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    let options = Options {
        resolution_width: opts["resolution_width"].as_u64().map(|n| n as u32),
        resolution_height: opts["resolution_height"].as_u64().map(|n| n as u32),
        demo: opts["demo"].as_bool().unwrap_or(false),
        include_default_user_jvm: opts["include_default_user_jvm"].as_bool().unwrap_or(true),
        extra_jvm_args: &jvm,
        extra_game_args: &game,
        ..Options::default()
    };
    let result = launch::build(Input {
        metadata: &md,
        install: &install,
        runtime: &rt,
        root_directory: "/fixture root",
        game_directory: "/fixture game",
        natives_directory: "/fixture natives",
        auth: Auth {
            player_name: "Fake Player",
            uuid: "fake-uuid",
            access_token: "fake-token",
            session: Some("fake-session"),
            user_type: "msa",
            xuid: "",
            client_id: "",
            user_properties: "{}",
        },
        options,
        force_java: force,
    });
    let actual = match result {
        Ok(p) => {
            if !oracle {
                use launch::process::{Event, RecordEnd, Stream};
                let mut bytes = Vec::new();
                let mut writer = launch::events::EventWriter::new(
                    &mut bytes,
                    "fake-id",
                    Source::Official,
                    "fixture",
                    &p,
                );
                writer.emit(Event::Started { pid: 42 });
                writer.emit(Event::Log {
                    stream: Stream::Stdout,
                    bytes: b"out\0",
                    end: RecordEnd::Newline,
                });
                writer.emit(Event::Log {
                    stream: Stream::Stderr,
                    bytes: b"err",
                    end: RecordEnd::Eof,
                });
                assert!(!writer.output_failed());
                let lines = String::from_utf8(bytes).unwrap();
                assert!(!lines.contains("fake-token"));
                assert!(!lines.contains("fake-session"));
                assert!(lines.contains(
                    r#""sequence":1,"encoding":"base64","end":"newline","data":"b3V0AA==""#
                ));
                assert!(
                    lines.contains(r#""sequence":2,"encoding":"base64","end":"eof","data":"ZXJy""#)
                );
            }

            json!({"java_executable":p.java_executable(),"working_directory":p.working_directory,"jvm_args":p.jvm_args().collect::<Vec<_>>(),"main_class":p.main_class(),"game_args":p.game_args().collect::<Vec<_>>(),"argv":p.argv().collect::<Vec<_>>(),"redacted_argv":p.redacted_argv().collect::<Vec<_>>(),"sensitive_argv_indices":p.sensitive_argv_indices})
        }
        Err(e) => json!({"error":e}),
    };
    if oracle {
        let fixture = json!({"metadata":md,"install":install,"runtime":rt,"root_directory":"/fixture root","game_directory":"/fixture game","natives_directory":"/fixture natives","auth":{"player_name":"Fake Player","uuid":"fake-uuid","access_token":"fake-token","session":"fake-session"},"options":opts,"force_java":force});
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), fixture.to_string()).unwrap();
        let out = std::process::Command::new(std::env::var("JMCL_ZIG_LAUNCH_ORACLE").unwrap())
            .arg(tmp.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(actual, expected, "fixture {fixture}");
    } else {
        assert!(actual.get("error").is_none());
        assert_eq!(
            actual["redacted_argv"][actual["sensitive_argv_indices"][0].as_u64().unwrap() as usize],
            "<redacted>"
        );
    }
}
#[test]
fn native_plan_smoke() {
    check(base(), json!({}), false, 17, Some("x86_64"), false)
}
#[test]
#[ignore = "requires pinned Zig launch fixture adapter"]
fn exact_oracle_plans() {
    for options in [
        json!({}),
        json!({"include_default_user_jvm":false}),
        json!({"resolution_width":0,"resolution_height":720,"extra_jvm_args":["-Dfake=fake-token","${literal}","a b"],"extra_game_args":["fake-session",";echo no"]}),
    ] {
        check(base(), options, false, 17, Some("x86_64"), true)
    }
    for (force, major, arch) in [
        (false, 8, Some("x86_64")),
        (false, 17, None),
        (false, 17, Some("arm64")),
        (true, 8, None),
    ] {
        check(base(), json!({}), force, major, arch, true)
    }
    for token in [
        "${unknown}",
        "${}",
        "${unfinished",
        "x${auth_access_token}y",
        "$literal",
        "${classpath_separator}",
        "${game_assets}",
    ] {
        let mut md = base();
        md["arguments"]["game"] = json!([token]);
        check(md, json!({}), false, 17, Some("x86_64"), true)
    }
    for legacy in [
        "--name '${auth_player_name}' --empty ''",
        "--token ${auth_session}",
        "a\\ b \"x\\\"y\"",
        "unterminated'",
        "trailing\\",
    ] {
        let mut md = base();
        md.as_object_mut().unwrap().remove("arguments");
        md["minecraftArguments"] = json!(legacy);
        check(md, json!({}), false, 17, Some("x86_64"), true)
    }
    for rule in [
        json!({"action":"allow","os":{"arch":"^amd64$"}}),
        json!({"action":"allow","os":{"arch":"a+"}}),
        json!({"action":"allow","os":{"arch":"z["}}),
        json!({"action":"allow","os":{"versionRange":{"min":"6.8","max":"6.8.0"}}}),
        json!({"action":"allow","features":{"unknown":false}}),
        json!({"action":"allow","features":{"has_custom_resolution":true}}),
    ] {
        let mut md = base();
        md["arguments"]["game"] =
            json!([{"rules":[rule],"value":["matched","${auth_access_token}"]}]);
        check(md, json!({}), false, 17, Some("x86_64"), true)
    }
}

#[test]
#[ignore = "requires pinned Zig launch fixture adapter"]
fn extra_plan_edges() {
    for v in [
        "-0",
        "+6.8",
        "6__0",
        "6._8",
        "++6",
        "6..8",
        "18446744073709551616",
    ] {
        let mut md = base();
        md["arguments"]["game"] =
            json!([{"rules":[{"action":"allow","os":{"versionRange":{"min":v}}}],"value":"yes"}]);
        check(md, json!({}), false, 17, Some("x86_64"), true)
    }
    for os in ["windows", "linux", "osx"] {
        let mut md = base();
        md["assets"] = json!("legacy");
        md["arguments"]["game"] = json!([
            "${game_assets}",
            "${library_directory}",
            "${assets_index_name}"
        ]);
        md["logging"] = json!({"client":{"argument":"-Dlog=${path}:${path}:${unknown}","file":{"id":"log.xml","sha1":"2".repeat(40),"size":0,"url":"https://example.invalid/log"},"type":"log4j2-xml"}});
        check(
            md,
            json!({"__virtual":true,"__target_os":os}),
            false,
            17,
            Some("x86_64"),
            true,
        )
    }
}

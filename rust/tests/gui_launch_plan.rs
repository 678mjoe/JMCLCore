//! GUI launch construction only: normalized in-memory metadata -> install plan ->
//! launch plan -> EventWriter. All credentials, paths, URLs and PIDs are fictional.
//! No RPC handler, runtime resolver/probe, preparation, process, file or transport
//! is invoked. EventWriter consumes already-redacted logs; this is not a test of
//! process-output redaction or of the launch.plan RPC result serializer.
use jmcl_core_rust::{
    java::detect::{CandidateSource, Runtime},
    launch::{
        self, Auth, Input, Options,
        events::EventWriter,
        process::{Event, RecordEnd, Stream},
    },
    metadata::{self, Source},
    plan::{self, Target},
};
use serde_json::{Value, json};
use std::io::{self, Write};

const ROOT: &str = "/fictional GUI instance/.minecraft";
const NATIVES: &str = "/fictional GUI instance/.minecraft/.jmcl/runtime/natives";
const JAVA: &str = "/fictional java/bin/java";
const TOKEN: &str = "synthetic-access-token";
const SESSION: &str = "synthetic-session";

// Path joining uses the host's separator, whereas the classpath delimiter is
// selected by the plan target. Neither path is resolved or opened by this test.
fn path(root: &str, tail: &str) -> String {
    format!("{root}{}{tail}", std::path::MAIN_SEPARATOR)
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}
fn runtime() -> Runtime {
    Runtime {
        executable: JAVA.into(),
        source: CandidateSource::Explicit,
        major_version: 21,
        specification_version: "21".into(),
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
fn auth<'a>(token: &'a str, session: Option<&'a str>) -> Auth<'a> {
    Auth {
        player_name: "Fictional Player",
        uuid: "00000000000000000000000000000000",
        access_token: token,
        session,
        user_type: "msa",
        xuid: "fictional-xuid",
        client_id: "fictional-client",
        user_properties: r#"{"fixture":["value with spaces"]}"#,
    }
}
fn base() -> Value {
    json!({
        "id": "gui-fixture", "type": "release",
        "mainClass": "net.minecraft.client.main.Main", "libraries": [],
        "downloads": {"client": {
            "sha1": "1".repeat(40), "size": 10,
            "url": "https://example.invalid/client.jar"
        }},
        "javaVersion": {"component": "fictional-runtime", "majorVersion": 21},
        "arguments": {"jvm": ["-cp", "${classpath}"], "game": []}
    })
}
fn library(name: &str, file: &str, hash: &str) -> Value {
    json!({"name": name, "downloads": {"artifact": {
        "path": file, "sha1": hash.repeat(40), "size": 1,
        "url": format!("https://example.invalid/{file}")
    }}})
}
fn logging() -> Value {
    json!({"client": {
        "argument": "-Dlog4j.configurationFile=${path}", "type": "log4j2-xml",
        "file": {"id": "client.xml", "sha1": "2".repeat(40), "size": 2,
            "url": "https://example.invalid/client.xml"}
    }})
}
fn log_arg() -> String {
    format!(
        "-Dlog4j.configurationFile={}",
        path(&path(&path(ROOT, "assets"), "log_configs"), "client.xml")
    )
}
fn inputs(raw: Value, assets: Option<Value>, os: &str) -> (Value, plan::Plan) {
    let md = metadata::metadata(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let assets = assets.map(|v| metadata::asset_index(&serde_json::to_vec(&v).unwrap()).unwrap());
    let install = plan::build(
        &md,
        assets.as_ref(),
        Source::Official,
        Target {
            os: os.into(),
            arch: "x86_64".into(),
            os_version: Some("6.8.0".into()),
        },
    )
    .unwrap();
    (md, install)
}
fn build(
    md: &Value,
    install: &plan::Plan,
    rt: &Runtime,
    auth: Auth<'_>,
    options: Options<'_>,
    force_java: bool,
) -> Result<launch::Plan, launch::Error> {
    launch::build(Input {
        metadata: md,
        install,
        runtime: rt,
        root_directory: ROOT,
        game_directory: ROOT,
        natives_directory: NATIVES,
        auth,
        options,
        force_java,
    })
}

#[derive(Default)]
struct MemoryWriter {
    bytes: Vec<u8>,
    flushes: usize,
}
impl Write for MemoryWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

// Expectations are supplied independently of the Plan: a missing/reordered argv
// entry or a shifted/duplicate sensitivity index changes the expected envelope.
fn assert_plan(
    p: &launch::Plan,
    expected: &[String],
    jvm_len: usize,
    sensitive: &[usize],
    secrets: &[&str],
    source: Source,
) {
    assert_eq!(p.argv().collect::<Vec<_>>(), expected);
    assert_eq!(p.java_executable(), expected[0]);
    assert_eq!(p.jvm_args().collect::<Vec<_>>(), expected[1..1 + jvm_len]);
    assert_eq!(p.main_class(), expected[1 + jvm_len]);
    assert_eq!(p.game_args().collect::<Vec<_>>(), expected[2 + jvm_len..]);
    assert_eq!(p.working_directory, ROOT);
    assert_eq!(p.sensitive_argv_indices, sensitive);
    let redacted: Vec<&str> = expected
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            if sensitive.contains(&i) {
                "<redacted>"
            } else {
                arg.as_str()
            }
        })
        .collect();
    assert_eq!(p.redacted_argv().collect::<Vec<_>>(), redacted);

    let mut sink = MemoryWriter::default();
    {
        let mut writer = EventWriter::new(&mut sink, "gui-request", source, "gui-fixture", p);
        writer.emit(Event::Started { pid: 4242 });
        // These are synthetic, already-redacted bytes, not output from a process.
        writer.emit(Event::Log {
            stream: Stream::Stdout,
            bytes: b"ok\0\xff",
            end: RecordEnd::Newline,
        });
        writer.emit(Event::Log {
            stream: Stream::Stderr,
            bytes: b"<redacted>\r",
            end: RecordEnd::More,
        });
        writer.emit(Event::Log {
            stream: Stream::Stdout,
            bytes: b"",
            end: RecordEnd::Eof,
        });
        assert!(!writer.output_failed());
    }
    let source = serde_json::to_string(&source).unwrap();
    let prefix = r#"{"protocol":1,"id":"gui-request","event":"#;
    let expected_wire = format!(
        "{prefix}\"started\",\"source\":{source},\"started\":{{\"stage\":\"launch\",\"version_id\":\"gui-fixture\",\"pid\":4242,\"redacted_argv\":{}}}}}\n\
         {prefix}\"stdout\",\"source\":{source},\"stdout\":{{\"sequence\":1,\"encoding\":\"base64\",\"end\":\"newline\",\"data\":\"b2sA/w==\"}}}}\n\
         {prefix}\"stderr\",\"source\":{source},\"stderr\":{{\"sequence\":2,\"encoding\":\"base64\",\"end\":\"more\",\"data\":\"PHJlZGFjdGVkPg0=\"}}}}\n\
         {prefix}\"stdout\",\"source\":{source},\"stdout\":{{\"sequence\":3,\"encoding\":\"base64\",\"end\":\"eof\",\"data\":\"\"}}}}\n",
        serde_json::to_string(&redacted).unwrap(),
    );
    let actual = String::from_utf8(sink.bytes).unwrap();
    assert_eq!(actual, expected_wire);
    assert_eq!(sink.flushes, 4);
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        assert!(
            !actual.contains(secret),
            "synthetic secret leaked into event"
        );
    }
    // Serializing diagnostics must not mutate the private process argv.
    assert_eq!(p.argv().collect::<Vec<_>>(), expected);
}

#[test]
fn modern_vanilla_gui_options_preserve_full_order_and_redaction() {
    let mut md = base();
    let first = library("example:first:1", "example/first/1/first-1.jar", "3");
    let mut excluded = library("example:excluded:1", "excluded.jar", "4");
    excluded["rules"] = json!([{"action": "allow", "os": {"name": "windows"}}]);
    md["libraries"] = json!([
        first,
        excluded,
        first,
        library("example:second:1", "example/second/1/second-1.jar", "5")
    ]);
    md["logging"] = logging();
    md["assets"] = json!("fictional-assets");
    md["arguments"] = json!({
        "default-user-jvm": ["-Xmx2G", "-Ddefault.auth=${auth_access_token}"],
        "jvm": [
            "-Djava.library.path=${natives_directory}",
            "-Dminecraft.launcher.brand=${launcher_name}",
            "-Dminecraft.launcher.version=${launcher_version}", "-cp", "${classpath}",
            {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": "excluded"}
        ],
        "game": [
            "--username", "${auth_player_name}", "--version", "${version_name}",
            "--gameDir", "${game_directory}", "--assetsDir", "${assets_root}",
            "--assetIndex", "${assets_index_name}", "--uuid", "${auth_uuid}",
            "--accessToken", "${auth_access_token}", "--userType", "${user_type}",
            "--versionType", "${version_type}", "--clientId", "${clientid}", "--xuid", "${auth_xuid}",
            {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}],
             "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]},
            {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}
        ]
    });
    let (md, install) = inputs(md, None, "linux");
    assert_eq!(
        install.classpath,
        [
            "libraries/example/first/1/first-1.jar",
            "libraries/example/second/1/second-1.jar",
            "versions/gui-fixture/gui-fixture.jar"
        ]
    );
    assert!(install.native_extractions.is_empty());
    assert_eq!(install.loader, "none");
    let classpath = [
        path(ROOT, "libraries/example/first/1/first-1.jar"),
        path(ROOT, "libraries/example/second/1/second-1.jar"),
        path(ROOT, "versions/gui-fixture/gui-fixture.jar"),
    ]
    .join(":");
    let p = build(
        &md,
        &install,
        &runtime(),
        auth(TOKEN, Some(SESSION)),
        Options {
            resolution_width: Some(1280),
            resolution_height: Some(720),
            demo: true,
            launcher_name: "JMCL GUI",
            launcher_version: "fictional-1",
            extra_jvm_args: &[
                "-Xmx4G",
                "-Dliteral=${auth_access_token}",
                "-Dextra=synthetic-access-token",
            ],
            extra_game_args: &[
                "--custom",
                "value with spaces",
                "prefix-synthetic-session-suffix",
                ";echo literal",
            ],
            ..Options::default()
        },
        false,
    )
    .unwrap();
    let expected = strings(&[
        JAVA,
        "-Xmx2G",
        "-Ddefault.auth=synthetic-access-token",
        &format!("-Djava.library.path={NATIVES}"),
        "-Dminecraft.launcher.brand=JMCL GUI",
        "-Dminecraft.launcher.version=fictional-1",
        "-cp",
        &classpath,
        "-Xmx4G",
        "-Dliteral=${auth_access_token}",
        "-Dextra=synthetic-access-token",
        &log_arg(),
        "net.minecraft.client.main.Main",
        "--username",
        "Fictional Player",
        "--version",
        "gui-fixture",
        "--gameDir",
        ROOT,
        "--assetsDir",
        &path(ROOT, "assets"),
        "--assetIndex",
        "fictional-assets",
        "--uuid",
        "00000000000000000000000000000000",
        "--accessToken",
        TOKEN,
        "--userType",
        "msa",
        "--versionType",
        "release",
        "--clientId",
        "fictional-client",
        "--xuid",
        "fictional-xuid",
        "--width",
        "1280",
        "--height",
        "720",
        "--demo",
        "--custom",
        "value with spaces",
        "prefix-synthetic-session-suffix",
        ";echo literal",
    ]);
    assert_plan(
        &p,
        &expected,
        11,
        &[2, 10, 26, 42],
        &[TOKEN, SESSION],
        Source::Official,
    );
}

#[test]
fn legacy_gui_arguments_keep_quoted_values_and_planned_virtual_assets() {
    let mut md = base();
    md.as_object_mut().unwrap().remove("arguments");
    md.as_object_mut().unwrap().remove("javaVersion");
    md["assets"] = json!("ignored-fallback");
    md["assetIndex"] = json!({"id": "legacy", "sha1": "6".repeat(40), "size": 3,
        "totalSize": 4, "url": "https://example.invalid/assets.json"});
    md["logging"] = logging();
    md["minecraftArguments"] = json!(concat!(
        "--username '${auth_player_name}' --session ${auth_session} ",
        "--gameDir \"${game_directory}\" --assetsDir \"${game_assets}\" ",
        "--empty '' --escaped a\\ b --quote \"x\\\"y\" ",
        "--properties '${user_properties}' --joined x${auth_access_token}y"
    ));
    let (md, install) = inputs(
        md,
        Some(json!({
            "virtual": true, "map_to_resources": true,
            "objects": {"sounds/example.ogg": {"hash": "7".repeat(40), "size": 4}}
        })),
        "linux",
    );
    assert_eq!(
        install.asset_projections,
        [
            plan::AssetProjection {
                source_path: format!("assets/objects/77/{}", "7".repeat(40)),
                destination_path: "assets/virtual/legacy/sounds/example.ogg".into(),
                kind: "virtual_assets".into()
            },
            plan::AssetProjection {
                source_path: format!("assets/objects/77/{}", "7".repeat(40)),
                destination_path: "resources/sounds/example.ogg".into(),
                kind: "resources".into()
            },
        ]
    );
    let mut rt = runtime();
    rt.major_version = 8;
    let p = build(
        &md,
        &install,
        &rt,
        auth(TOKEN, Some(SESSION)),
        Options {
            extra_jvm_args: &["-Xmx1G"],
            extra_game_args: &["literal ${auth_session}", "--tail"],
            ..Options::default()
        },
        false,
    )
    .unwrap();
    let expected = strings(&[
        JAVA,
        &format!("-Djava.library.path={NATIVES}"),
        "-cp",
        &path(ROOT, "versions/gui-fixture/gui-fixture.jar"),
        "-Xmx1G",
        &log_arg(),
        "net.minecraft.client.main.Main",
        "--username",
        "Fictional Player",
        "--session",
        SESSION,
        "--gameDir",
        ROOT,
        "--assetsDir",
        &path(&path(&path(ROOT, "assets"), "virtual"), "legacy"),
        "--empty",
        "",
        "--escaped",
        "a b",
        "--quote",
        "x\"y",
        "--properties",
        r#"{"fixture":["value with spaces"]}"#,
        "--joined",
        "xsynthetic-access-tokeny",
        "literal ${auth_session}",
        "--tail",
    ]);
    assert_plan(
        &p,
        &expected,
        5,
        &[10, 24],
        &[TOKEN, SESSION],
        Source::Official,
    );
}

#[test]
fn overlapping_equal_missing_and_empty_sessions_redact_whole_arguments_once() {
    for (token, session) in [
        (TOKEN, Some("synthetic-access-token-session")),
        ("synthetic-session-token", Some("synthetic-session")),
        (TOKEN, Some(TOKEN)),
        (TOKEN, None),
        (TOKEN, Some("")),
        ("虚构令牌", Some("虚构令牌会话")),
    ] {
        let mut md = base();
        md["arguments"] = json!({
            "jvm": ["-Dauth=${auth_access_token}:${auth_session}", "-cp", "${classpath}"],
            "game": ["${auth_access_token}", "${auth_session}"]
        });
        let (md, install) = inputs(md, None, "linux");
        let effective_session = session.unwrap_or(token);
        let combined = format!("pre-{token}-{effective_session}-{token}-post");
        let p = build(
            &md,
            &install,
            &runtime(),
            auth(token, session),
            Options {
                extra_jvm_args: &[&combined, "literal ${auth_access_token}"],
                extra_game_args: &[&combined, "safe with spaces"],
                ..Options::default()
            },
            false,
        )
        .unwrap();
        let expected = strings(&[
            JAVA,
            &format!("-Dauth={token}:{effective_session}"),
            "-cp",
            &path(ROOT, "versions/gui-fixture/gui-fixture.jar"),
            &combined,
            "literal ${auth_access_token}",
            "net.minecraft.client.main.Main",
            token,
            effective_session,
            &combined,
            "safe with spaces",
        ]);
        assert_plan(
            &p,
            &expected,
            5,
            &[1, 4, 7, 8, 9],
            &[token, effective_session],
            Source::Bmclapi,
        );
    }
}

#[test]
fn disabling_default_jvm_and_target_classpath_delimiters_do_not_shift_secrets() {
    for (os, delimiter) in [("linux", ":"), ("osx", ":"), ("windows", ";")] {
        let mut md = base();
        md["libraries"] = json!([library("example:first:1", "first.jar", "3")]);
        md["arguments"]["default-user-jvm"] = json!(["-Ddisabled=${auth_access_token}"]);
        md["arguments"]["game"] = json!(["${classpath_separator}", "${auth_session}"]);
        let (md, install) = inputs(md, None, os);
        let p = build(
            &md,
            &install,
            &runtime(),
            auth(TOKEN, None),
            Options {
                include_default_user_jvm: false,
                ..Options::default()
            },
            false,
        )
        .unwrap();
        let classpath = [
            path(ROOT, "libraries/first.jar"),
            path(ROOT, "versions/gui-fixture/gui-fixture.jar"),
        ]
        .join(delimiter);
        let expected = strings(&[
            JAVA,
            "-cp",
            &classpath,
            "net.minecraft.client.main.Main",
            delimiter,
            TOKEN,
        ]);
        assert_plan(&p, &expected, 2, &[5], &[TOKEN], Source::Official);
    }
}

#[test]
fn gui_features_and_last_matching_rules_control_only_selected_arguments() {
    let mut md = base();
    md["arguments"]["game"] = json!([
        {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}],
         "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]},
        {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
        {"rules": [{"action": "allow", "features": {"has_quick_plays_support": true}}],
         "value": ["--quickPlayPath", "${quickPlayPath}"]},
        {"rules": [{"action": "allow", "features": {"is_quick_play_singleplayer": true}}],
         "value": ["--quickPlaySingleplayer", "${quickPlaySingleplayer}"]},
        {"rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "linux"}}], "value": "${unknown}"},
        {"rules": [{"action": "disallow"}, {"action": "allow", "os": {"name": "linux"}}], "value": "${auth_access_token}"}
    ]);
    let (md, install) = inputs(md, None, "linux");
    for complete_resolution in [false, true] {
        let p = build(
            &md,
            &install,
            &runtime(),
            auth(TOKEN, None),
            Options {
                resolution_width: Some(0),
                resolution_height: complete_resolution.then_some(720),
                quick_play_support: true,
                quick_play_path: "fictional quick play.json",
                quick_play_singleplayer: "Fictional World",
                ..Options::default()
            },
            false,
        )
        .unwrap();
        let mut expected = strings(&[
            JAVA,
            "-cp",
            &path(ROOT, "versions/gui-fixture/gui-fixture.jar"),
            "net.minecraft.client.main.Main",
        ]);
        if complete_resolution {
            expected.extend(strings(&["--width", "0", "--height", "720"]));
        }
        expected.extend(strings(&[
            "--quickPlayPath",
            "fictional quick play.json",
            "--quickPlaySingleplayer",
            "Fictional World",
            TOKEN,
        ]));
        assert_plan(
            &p,
            &expected,
            2,
            &[if complete_resolution { 12 } else { 8 }],
            &[TOKEN],
            Source::Official,
        );
    }
}

#[test]
fn manual_runtime_checks_and_forced_override_do_not_probe_java() {
    let (md, install) = inputs(base(), None, "linux");
    for (major, arch, error) in [
        (8, Some("x86_64"), "JavaVersionIncompatible"),
        (21, Some("arm64"), "JavaArchitectureIncompatible"),
        (21, None, "JavaArchitectureIncompatible"),
    ] {
        let mut rt = runtime();
        rt.major_version = major;
        rt.architecture = arch.map(Into::into);
        assert_eq!(
            build(
                &md,
                &install,
                &rt,
                auth(TOKEN, None),
                Options::default(),
                false
            )
            .err(),
            Some(error)
        );
        let p = build(
            &md,
            &install,
            &rt,
            auth(TOKEN, None),
            Options::default(),
            true,
        )
        .unwrap();
        let expected = strings(&[
            JAVA,
            "-cp",
            &path(ROOT, "versions/gui-fixture/gui-fixture.jar"),
            "net.minecraft.client.main.Main",
        ]);
        assert_plan(&p, &expected, 2, &[], &[TOKEN], Source::Official);
    }
}

#[test]
fn invalid_auth_placeholders_and_legacy_syntax_fail_during_construction() {
    let (md, install) = inputs(base(), None, "linux");
    for field in ["player_name", "uuid", "access_token"] {
        let mut auth = auth(TOKEN, None);
        match field {
            "player_name" => auth.player_name = "",
            "uuid" => auth.uuid = "",
            _ => auth.access_token = "",
        }
        assert_eq!(
            build(&md, &install, &runtime(), auth, Options::default(), true).err(),
            Some("InvalidAuthSession")
        );
    }
    for (argument, error) in [
        ("${unknown}", "UnknownPlaceholder"),
        ("${}", "InvalidPlaceholder"),
        ("prefix${unfinished", "InvalidPlaceholder"),
    ] {
        let mut md = base();
        md["arguments"]["game"] = json!([argument]);
        let (md, install) = inputs(md, None, "linux");
        assert_eq!(
            build(
                &md,
                &install,
                &runtime(),
                auth(TOKEN, None),
                Options::default(),
                false
            )
            .err(),
            Some(error)
        );
    }
    for argument in ["unterminated'", "trailing\\"] {
        let mut md = base();
        md.as_object_mut().unwrap().remove("arguments");
        md["minecraftArguments"] = json!(argument);
        let (md, install) = inputs(md, None, "linux");
        assert_eq!(
            build(
                &md,
                &install,
                &runtime(),
                auth(TOKEN, None),
                Options::default(),
                false
            )
            .err(),
            Some("InvalidLegacyArguments")
        );
    }
}

#[test]
fn gui_event_output_failure_is_sticky_without_retrying_or_spawning() {
    struct FlushFailure {
        writes: usize,
        flushes: usize,
    }
    impl Write for FlushFailure {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "fictional GUI sink closed",
            ))
        }
    }
    let (md, install) = inputs(base(), None, "linux");
    let p = build(
        &md,
        &install,
        &runtime(),
        auth(TOKEN, None),
        Options::default(),
        false,
    )
    .unwrap();
    let mut sink = FlushFailure {
        writes: 0,
        flushes: 0,
    };
    let mut writer = EventWriter::new(
        &mut sink,
        "gui-request",
        Source::Official,
        "gui-fixture",
        &p,
    );
    writer.emit(Event::Started { pid: 4242 });
    assert!(writer.output_failed());
    let writes = writer.writer.writes;
    assert!(writes > 0);
    writer.emit(Event::Started { pid: 9999 });
    writer.emit(Event::Log {
        stream: Stream::Stdout,
        bytes: b"ignored",
        end: RecordEnd::Eof,
    });
    assert_eq!(writer.writer.writes, writes);
    assert_eq!(writer.writer.flushes, 1);
}

use super::*;
fn raw(s: &str) -> Box<RawValue> {
    RawValue::from_string(s.into()).unwrap()
}
#[test]
fn complete_schema_defaults_bytes_nulls_and_error_order() {
    let p = parse(Some(&raw(
        r#"{"id":[255],"java_paths":[[254],""],"future":1e400}"#,
    )))
    .unwrap();
    assert_eq!(p.id, [255]);
    assert_eq!(p.directory, b".minecraft");
    assert_eq!(p.store, b".jmclcore");
    assert_eq!((p.workers, p.retries), (8, 2));
    for s in [
        r#"{"id":"","java_paths":null}"#,
        r#"{"id":"","workers":null}"#,
        r#"{"id":"v","source":null}"#,
        r#"{"id":"v","directory":null}"#,
        r#"{"id":"v","java_policy":null}"#,
        r#"{"id":"v","java_paths":[true]}"#,
    ] {
        assert_eq!(
            parse(Some(&raw(s))).unwrap_err().message,
            "install.execute parameters are invalid"
        );
    }
    assert_eq!(
        parse(Some(&raw(r#"{"id":"","java_policy":[255]}"#)))
            .unwrap_err()
            .message,
        "install.execute parameters are out of range"
    );
    assert_eq!(
        parse(Some(&raw(r#"{"id":"v","java_policy":[255]}"#)))
            .unwrap_err()
            .message,
        "java_policy must be auto, local, or managed"
    );
    let p=parse(Some(&raw(r#"{"id":"v","os":null,"arch":null,"workers":"3_2","retries":5.0,"java_policy":[97,117,116,111]}"#))).unwrap();
    assert_eq!((p.workers, p.retries), (32, 5));
}
#[test]
fn loader_classification_excludes_selected_malformed_conflicting_and_duplicate_fields() {
    for s in [
        r#"{"id":"v","forge_version":""}"#,
        r#"{"id":"v","fabric_loader":false}"#,
        r#"{"id":"v","forge_version":"a","neoforge_version":"b"}"#,
        r#"{"id":"v","fabric_loader":null,"fabric_loader":null}"#,
    ] {
        assert!(!selected(Some(&raw(s))));
    }
    assert!(selected(Some(&raw(
        r#"{"id":"v","fabric_loader":null,"forge_version":null,"neoforge_version":null}"#
    ))));
}

#[test]
fn ordinary_string_request_matches_existing_decoder_and_retains_range_errors() {
    for s in [
        r#"{"id":"v"}"#,
        r#"{"id":"版本","source":"bmclapi","os":"linux","arch":"x86_64","directory":"owned-root","store_directory":"owned-cas","workers":1,"retries":0,"java_paths":["opaque-command"],"java_policy":"local","fabric_loader":null}"#,
    ] {
        let r = raw(s);
        let p = parse(Some(&r)).unwrap();
        let super::super::rpc::Request::Execute(old) =
            super::super::rpc::parse("install.execute", Some(&r)).unwrap()
        else {
            panic!()
        };
        assert_eq!(p.id, old.prepare.selection.id.as_bytes());
        assert_eq!(p.source, old.prepare.selection.source);
        assert_eq!(p.os, old.prepare.selection.os);
        assert_eq!(p.arch, old.prepare.selection.arch);
        assert_eq!((p.workers, p.retries), (old.workers, old.retries));
        assert_eq!(
            crate::store::path_from_bytes(&p.directory).unwrap(),
            old.prepare.directory.path().unwrap()
        );
        assert_eq!(
            crate::store::path_from_bytes(&p.store).unwrap(),
            old.store_directory.path().unwrap()
        );
    }
    for s in [
        r#"{"id":"v","workers":0}"#,
        r#"{"id":"v","workers":33}"#,
        r#"{"id":"v","retries":6}"#,
        r#"{"id":"v","store_directory":""}"#,
    ] {
        assert_eq!(
            parse(Some(&raw(s))).unwrap_err().message,
            "install.execute parameters are out of range"
        );
    }
}

#[test]
fn managed_cli_presentation_notice_and_raw_identity_are_pure() {
    use crate::plan::CompletionPresentation;
    let mut out = vec![];
    let mut err = vec![];
    let mut cli = Cli {
        out: &mut out,
        err: &mut err,
        last_completed: 0,
    };
    cli.event(ByteEvent::Retry(ByteRetryEvent {
        path: BytePath(vec![255]),
        attempt: 1,
        max_attempts: 3,
        error_name: "DownloadHashMismatch".into(),
    }))
    .unwrap();
    for completed in [1, 2, 2] {
        cli.event(ByteEvent::Progress(Progress {
            files_completed: completed,
            files_total: 2,
            bytes_verified: 4,
            bytes_total: 4,
            ..Default::default()
        }))
        .unwrap();
    }
    cli.completed(
        crate::plan::CompletionInfo {
            version: &[255],
            source: Source::Bmclapi,
            files: 0,
            bytes: 0,
        },
        &Summary::default(),
    )
    .unwrap();
    assert_eq!(out,b"# source\tBMCLAPI\nversion\t\xff\ndownloaded\t0\ncached\t0\nhardLinked\t0\ncopied\t0\nverifiedBytes\t0\ntransferredBytes\t0\n");
    assert_eq!(err,b"retry 1/3: \xff (DownloadHashMismatch)\ndownload progress: 2/2 files, 4/4 bytes verified\n");
}

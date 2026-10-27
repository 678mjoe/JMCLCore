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

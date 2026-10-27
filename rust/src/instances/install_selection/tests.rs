use super::*;
#[test]
fn classifier_preserves_ordered_validation_and_legacy_fallback_errors() {
    let valid = br#"{"id":"demo","name":"n","version_id":[255]}"#;
    assert!(matches!(parse(valid,"demo"),Ok(Selection::Vanilla{version,..}) if version==[255]));
    for raw in [
        br#"{"id":"demo","name":[255],"version_id":"v","forge_version":"f"}"#.as_slice(),
        br#"{"version_id":[255],"future":"\ud800"}"#,
        br#"{"version_id":[255],"forge_version":false}"#,
    ] {
        assert_ne!(vanilla(raw), Some(true));
        assert_eq!(parse(raw, "demo").err(), Manifest::parse(raw, "demo").err());
    }
    for (raw, error) in [
        (
            br#"{"id":"demo","name":"n","version_id":[255],"source":"bad"}"#.as_slice(),
            "InvalidEnumTag",
        ),
        (
            br#"{"id":"demo","name":"n","version_id":[255],"version_id":"v"}"#,
            "DuplicateField",
        ),
        (
            br#"{"id":"other","name":"n","version_id":[255]}"#,
            "InstanceIdMismatch",
        ),
    ] {
        assert_eq!(parse(raw, "demo").err(), Some(error));
    }
}

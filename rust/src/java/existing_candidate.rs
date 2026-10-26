//! Test-only proof of existing-receipt decoding and eligibility.
//! This is not wired to operational lookup, filesystem enumeration or probing.
use super::super::receipt;

#[derive(Debug, PartialEq, Eq)]
enum Candidate {
    Skip,
    Utf8Executable(String),
    // The source permits these bytes; operational path/probe support remains
    // separately unresolved, not silently reclassified as an invalid receipt.
    UnresolvedExecutableBytes(Vec<u8>),
}
fn from_buffer(
    bytes: &[u8],
    major: u16,
    os: &str,
    arch: &str,
    preferred_component: Option<&[u8]>,
    preferred_pass: bool,
) -> Result<Candidate, &'static str> {
    let document = receipt::parse_read(bytes)?;
    let platform = std::str::from_utf8(&document.platform.0)
        .ok()
        .and_then(receipt::platform_target)
        .ok_or("InvalidRuntimeReceipt")?;
    if document.major_version != major || platform != (os, arch) {
        return Ok(Candidate::Skip);
    }
    if preferred_pass {
        let Some(component) = preferred_component else {
            return Ok(Candidate::Skip);
        };
        if document.component.as_ref().map(|v| v.0.as_slice()) != Some(component) {
            return Ok(Candidate::Skip);
        }
    }
    Ok(match String::from_utf8(document.executable.0) {
        Ok(path) => Candidate::Utf8Executable(path),
        Err(bytes) => Candidate::UnresolvedExecutableBytes(bytes.into_bytes()),
    })
}
fn fixture() -> serde_json::Value {
    serde_json::json!({"provider":"mojang","id":"fixture","platform":"linux","major_version":17,"java_home":"","executable":"bin/java","source_url":"https://fixture.invalid/runtime","installed_at_ms":123,"component":"preferred"})
}
fn candidate(v: &serde_json::Value) -> Result<Candidate, &'static str> {
    from_buffer(
        &serde_json::to_vec(v).unwrap(),
        17,
        "linux",
        "x86_64",
        Some(b"preferred"),
        true,
    )
}
#[test]
fn buffer_grammar_accepts_source_receipts_without_lookup() {
    for (field, value) in [
        ("provider", serde_json::json!(0)),
        ("provider", serde_json::json!("+0")),
        ("id", serde_json::json!(b"fixture".to_vec())),
        ("platform", serde_json::json!(b"linux".to_vec())),
        ("executable", serde_json::json!(b"bin/java".to_vec())),
        ("component", serde_json::json!(b"preferred".to_vec())),
        ("vendor", serde_json::json!([255])),
        ("version", serde_json::json!(["65", 65.0])),
        (
            "source_url",
            serde_json::json!(b"https://fixture.invalid/runtime".to_vec()),
        ),
    ] {
        let mut v = fixture();
        v[field] = value;
        assert_eq!(
            receipt::parse(&serde_json::to_vec(&v).unwrap()),
            Err("InvalidRuntimeReceipt"),
            "{field}"
        );
        assert_eq!(
            candidate(&v),
            Ok(Candidate::Utf8Executable("bin/java".into())),
            "{field}"
        );
    }
    let raw = serde_json::to_vec(&fixture()).unwrap();
    assert_eq!(receipt::parse(&raw).unwrap().executable, "bin/java");
    assert_eq!(
        candidate(&fixture()),
        Ok(Candidate::Utf8Executable("bin/java".into()))
    );
}
#[test]
fn candidate_filters_preserve_two_pass_matching() {
    let v = fixture();
    let raw = serde_json::to_vec(&v).unwrap();
    for (major, os, arch, component, pass, eligible) in [
        (
            17,
            "linux",
            "x86_64",
            Some(b"preferred".as_slice()),
            true,
            true,
        ),
        (
            17,
            "linux",
            "x86_64",
            Some(b"Preferred".as_slice()),
            true,
            false,
        ),
        (17, "linux", "x86_64", None, true, false),
        (17, "linux", "x86_64", None, false, true),
        (
            17,
            "linux",
            "x86_64",
            Some(b"other".as_slice()),
            false,
            true,
        ),
        (8, "linux", "x86_64", None, false, false),
        (17, "osx", "x86_64", None, false, false),
        (17, "linux", "arm64", None, false, false),
    ] {
        assert_eq!(
            matches!(
                from_buffer(&raw, major, os, arch, component, pass).unwrap(),
                Candidate::Utf8Executable(_)
            ),
            eligible
        );
    }
    let mut v = v;
    v["component"] = serde_json::json!([255]);
    let raw = serde_json::to_vec(&v).unwrap();
    assert_eq!(
        from_buffer(&raw, 17, "linux", "x86_64", Some(&[255]), true),
        Ok(Candidate::Utf8Executable("bin/java".into()))
    );
    assert_eq!(
        from_buffer(&raw, 17, "linux", "x86_64", None, false),
        Ok(Candidate::Utf8Executable("bin/java".into()))
    );
}
#[test]
fn invalid_buffers_never_become_candidates() {
    for (field, value) in [
        ("provider", serde_json::json!(2)),
        ("provider", serde_json::json!(-0.0)),
        ("major_version", serde_json::json!(0)),
        ("id", serde_json::json!([255])),
        ("platform", serde_json::json!("other")),
        ("executable", serde_json::json!("../java")),
        ("executable", serde_json::json!(b"bin/../java".to_vec())),
        ("executable", serde_json::json!([98, 105, 110, 47, 0])),
        ("vendor", serde_json::json!([256])),
        (
            "source_url",
            serde_json::json!("http://fixture.invalid/runtime"),
        ),
    ] {
        let mut v = fixture();
        v[field] = value;
        assert_eq!(candidate(&v), Err("InvalidRuntimeReceipt"), "{field}");
    }
    let mut raw = serde_json::to_vec(&fixture()).unwrap();
    raw.pop();
    raw.extend_from_slice(br#", "unknown":"\ud800"}"#);
    assert_eq!(
        from_buffer(&raw, 17, "linux", "x86_64", None, false),
        Err("InvalidRuntimeReceipt")
    );
    for raw in [b"null".as_slice(), b"[]", b"{}", b"{"] {
        assert_eq!(
            from_buffer(raw, 17, "linux", "x86_64", None, false),
            Err("InvalidRuntimeReceipt")
        );
    }
}
#[test]
fn opaque_executable_is_explicitly_unresolved() {
    let mut v = fixture();
    v["executable"] = serde_json::json!([98, 105, 110, 47, 255]);
    assert_eq!(
        candidate(&v),
        Ok(Candidate::UnresolvedExecutableBytes(b"bin/\xff".to_vec()))
    );
}

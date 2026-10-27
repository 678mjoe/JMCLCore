use super::*;

#[test]
fn string_component_boundary_cannot_represent_source_valid_opaque_bytes() {
    // Pinned install/plan.zig validateComponent permits these high bytes.
    // The existing String entry point cannot receive them at all.
    for bytes in [b"v\xff".as_slice(), b"\x80", b"CON\xff.txt"] {
        assert!(std::str::from_utf8(bytes).is_err());
    }
    for text in ["1.21", "版本", "é", "COM0", "COM10", ".hidden", "a..b"] {
        assert_eq!(validate_component(text), Ok(()));
    }
    for text in [
        "", ".", "..", "a/", "a\\b", "a:b", "a\0b", "con.txt", "Lpt9.x",
    ] {
        assert_eq!(validate_component(text), Err(err("UnsafePath")));
    }
}

#[test]
fn byte_component_accepts_source_defined_high_bytes_without_path_conversion() {
    for bytes in [
        b"v\xff".as_slice(),
        b"\x80",
        b"\xc0\xaf",
        b"CON\xff.txt",
        b"COM0",
        b"COM10",
        b"LPT0",
        b".hidden",
        b"a..b",
        b"a b",
        b"*?<>|\"",
        "版本".as_bytes(),
        "é".as_bytes(),
    ] {
        assert_eq!(validate_component_bytes(bytes), Ok(()), "{bytes:?}");
    }
    // The source has no general component length cap (loader IDs do).
    assert_eq!(validate_component_bytes(&[b'a'; 257]), Ok(()));
}

#[test]
fn byte_component_rejects_source_forbidden_bytes_and_reserved_stems() {
    for bytes in [
        b"".as_slice(),
        b".",
        b"..",
        b"\xff.",
        b"\xff ",
        b"\xff/",
        b"\xff\\x",
        b"x:\xff",
        b"\xff\0",
        b"\xff\x7f",
        b"con",
        b"PrN.txt",
        b"aUx.any",
        b"nul",
        b"COM1.bin",
        b"lpt9.x",
        b"CON/\0.",
    ] {
        assert_eq!(
            validate_component_bytes(bytes),
            Err(err("UnsafePath")),
            "{bytes:?}"
        );
    }
    // Finite source rule domain, not generated paths or filesystem probes.
    for control in 0u8..0x20 {
        assert_eq!(
            validate_component_bytes(&[b'a', control, 0xff]),
            Err(err("UnsafePath"))
        );
    }
    for prefix in ["COM", "LPT"] {
        for digit in 1..=9 {
            assert_eq!(
                validate_component_bytes(format!("{prefix}{digit}.bin").as_bytes()),
                Err(err("UnsafePath"))
            );
        }
    }
}

#[test]
fn string_delegation_keeps_relative_and_loader_policies_separate() {
    for text in [
        "1.21", "版本", "é", "COM0", "COM10", ".hidden", "a..b", "*?<>|\"", "", ".", "..", "a/",
        "a\\b", "a:b", "a\0b", "con.txt", "Lpt9.x", "v.", "v ",
    ] {
        assert_eq!(
            validate_component(text),
            validate_component_bytes(text.as_bytes())
        );
    }
    for text in ["mod/path.jar", "版本/é", ".hidden/name"] {
        assert_eq!(validate_relative_path(text), Ok(()));
    }
    for text in [
        "", "../x", "a//b", "/x", "a/", "CON.txt", "a/b.", "a\\b", "x:y",
    ] {
        assert_eq!(validate_relative_path(text), Err(err("UnsafePath")));
    }
    // Loader profile policy is ASCII-only with a 256-byte cap, not this helper.
    for text in ["版本", "a b", "*", "v.", "", &"a".repeat(257)] {
        assert_eq!(
            crate::version::fabric::profile_id(text),
            Err(err("InvalidProfileId"))
        );
    }
    for text in ["1.21", "0.16.0", "a-_+b", "CON"] {
        assert_eq!(crate::version::fabric::profile_id(text), Ok(()));
    }
}

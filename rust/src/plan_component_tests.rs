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

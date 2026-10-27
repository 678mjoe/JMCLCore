//! Exact native text bytes: Unix bytes or Windows WTF-8, without replacement.
#[cfg(any(windows, test))]
pub(crate) fn from_utf16(units: impl IntoIterator<Item = u16>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in char::decode_utf16(units) {
        match value {
            Ok(c) => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            Err(e) => {
                let n = e.unpaired_surrogate();
                bytes.extend_from_slice(&[
                    (0xe0 | (n >> 12)) as u8,
                    (0x80 | ((n >> 6) & 63)) as u8,
                    (0x80 | (n & 63)) as u8,
                ]);
            }
        }
    }
    bytes
}
pub(crate) fn os_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        name.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        from_utf16(name.encode_wide())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_wtf8_finite_corpus_preserves_pairs_and_unpaired_units() {
        let cases: &[(&[u16], &[u8])] = &[
            (&[0x41], b"A"),
            (&[0x7248, 0x672c], "版本".as_bytes()),
            (&[0xd83d, 0xde00], "😀".as_bytes()),
            (&[0xd800], &[0xed, 0xa0, 0x80]),
            (&[0xdc00], &[0xed, 0xb0, 0x80]),
            (&[0xd800, 0xd800], &[0xed, 0xa0, 0x80, 0xed, 0xa0, 0x80]),
            (&[0xdc00, 0xdc00], &[0xed, 0xb0, 0x80, 0xed, 0xb0, 0x80]),
            (&[0xdc00, 0xd800], &[0xed, 0xb0, 0x80, 0xed, 0xa0, 0x80]),
            (
                &[0x41, 0xd800, 0x42, 0xdc00, 0xd83d, 0xde00],
                &[
                    0x41, 0xed, 0xa0, 0x80, 0x42, 0xed, 0xb0, 0x80, 0xf0, 0x9f, 0x98, 0x80,
                ],
            ),
        ];
        for (units, expected) in cases {
            assert_eq!(from_utf16(units.iter().copied()), *expected);
        }
    }
    #[cfg(unix)]
    #[test]
    fn unix_native_wrapper_preserves_raw_ff() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(os_bytes(std::ffi::OsStr::from_bytes(&[255])), [255]);
    }
    #[cfg(windows)]
    #[test]
    fn windows_native_wrapper_preserves_from_wide_surrogates() {
        use std::os::windows::ffi::OsStringExt;
        for units in [
            vec![0xd800],
            vec![0xdc00],
            vec![0xd83d, 0xde00],
            vec![0xdc00, 0xd800],
            vec![0x41, 0xd800, 0x42],
        ] {
            assert_eq!(
                os_bytes(&std::ffi::OsString::from_wide(&units)),
                from_utf16(units)
            );
        }
    }
}

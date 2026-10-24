//! Borrowed structural JSON; decoded text always belongs to a wiping buffer.
//! serde_json's RawValue/ignore_value path stores only container delimiters in
//! scratch, not decoded strings (audited 1.0.154). Never deserialize String/Value.
use zeroize::{Zeroize, Zeroizing};
#[derive(Clone, Copy)]
pub(crate) struct Json<'a>(pub &'a str);
impl<'a> Json<'a> {
    pub fn parse(s: &'a str) -> Result<Self, ()> {
        let _: &serde_json::value::RawValue = serde_json::from_str(s).map_err(|_| ())?;
        // RawValue deliberately does not check surrogate pairing. Zig's scanner
        // does, including ignored fields. Validate escapes without decoding or
        // allocating unknown (possibly secret) string content.
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] != b'\\' {
                i += 1;
                continue;
            }
            if b.get(i + 1) != Some(&b'u') {
                i += 2;
                continue;
            }
            let hex = |start: usize| -> Result<u16, ()> {
                let digits = s.get(start..start + 4).ok_or(())?;
                u16::from_str_radix(digits, 16).map_err(|_| ())
            };
            let code = hex(i + 2)?;
            i += 6;
            if (0xd800..=0xdbff).contains(&code) {
                if b.get(i..i + 2) != Some(b"\\u") {
                    return Err(());
                }
                if !(0xdc00..=0xdfff).contains(&hex(i + 2)?) {
                    return Err(());
                }
                i += 6;
            } else if (0xdc00..=0xdfff).contains(&code) {
                return Err(());
            }
        }
        Ok(Self(s.trim()))
    }
    fn take(s: &'a str) -> Result<(Self, &'a str), ()> {
        let mut stream =
            serde_json::Deserializer::from_str(s).into_iter::<&serde_json::value::RawValue>();
        let raw = stream.next().ok_or(())?.map_err(|_| ())?;
        Ok((Self(raw.get()), &s[stream.byte_offset()..]))
    }
    pub fn field(self, key: &str) -> Result<Option<Self>, ()> {
        let mut rest = self.0.strip_prefix('{').ok_or(())?.trim_start();
        let mut result = None;
        while !rest.starts_with('}') {
            let (name, tail) = Self::take(rest)?;
            rest = tail.trim_start().strip_prefix(':').ok_or(())?.trim_start();
            let (value, tail) = Self::take(rest)?;
            if name.string()?.as_str() == key {
                if result.is_some() {
                    return Err(());
                }
                result = Some(value);
            }
            rest = tail.trim_start();
            if rest.starts_with('}') {
                break;
            }
            rest = rest.strip_prefix(',').ok_or(())?.trim_start();
        }
        Ok(result)
    }
    pub fn required(self, key: &str) -> Result<Self, ()> {
        self.field(key)?.ok_or(())
    }
    pub fn array(self) -> Result<Vec<Self>, ()> {
        let mut rest = self.0.strip_prefix('[').ok_or(())?.trim_start();
        let mut out = Vec::new();
        while !rest.starts_with(']') {
            let (v, tail) = Self::take(rest)?;
            out.push(v);
            rest = tail.trim_start();
            if rest.starts_with(']') {
                break;
            }
            rest = rest.strip_prefix(',').ok_or(())?.trim_start();
        }
        Ok(out)
    }
    pub fn null(self) -> bool {
        self.0 == "null"
    }
    pub fn string(self) -> Result<Zeroizing<String>, ()> {
        let s = self
            .0
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or(())?;
        let mut out = Zeroizing::new(String::with_capacity(s.len()));
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                if c < ' ' || c == '"' {
                    return Err(());
                }
                out.push(c);
                continue;
            }
            let c = match chars.next().ok_or(())? {
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                'b' => '\x08',
                'f' => '\x0c',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'u' => {
                    fn hex(chars: &mut std::str::Chars<'_>) -> Result<u32, ()> {
                        let mut n = 0;
                        for _ in 0..4 {
                            n = n * 16 + chars.next().and_then(|c| c.to_digit(16)).ok_or(())?;
                        }
                        Ok(n)
                    }
                    let mut n = hex(&mut chars)?;
                    if (0xd800..=0xdbff).contains(&n) {
                        if chars.next() != Some('\\') || chars.next() != Some('u') {
                            return Err(());
                        }
                        let low = hex(&mut chars)?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(());
                        }
                        n = 0x10000 + ((n - 0xd800) << 10) + (low - 0xdc00);
                    }
                    char::from_u32(n).ok_or(())?
                }
                _ => return Err(()),
            };
            let mut bytes = [0; 4];
            out.push_str(c.encode_utf8(&mut bytes));
            bytes.zeroize();
        }
        Ok(out)
    }
    pub fn u64(self) -> Result<u64, ()> {
        let text = if self.0.starts_with('"') {
            self.string()?
        } else {
            Zeroizing::new(self.0.to_owned())
        };
        unsigned_text(&text)
    }
    pub fn u32(self) -> Result<u32, ()> {
        self.u64()?.try_into().map_err(|_| ())
    }
}

/// Typed response numbers follow Zig's binary128 coercion.
pub(crate) fn unsigned_text(text: &str) -> Result<u64, ()> {
    crate::json_number::unsigned_text(text, 64).map_err(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn float_underscore_syntax_is_distinct_from_integer_syntax() {
        for text in ["1__0.0", "1_.0", "1._0", "1e_0", "1e0_", "_1.0"] {
            assert!(unsigned_text(text).is_err(), "{text}");
        }
        assert_eq!(unsigned_text("1__0"), Ok(10));
    }
    #[test]
    fn zig_integer_string_coercion() {
        for (raw, n) in [
            (r#""1_280""#, 1280),
            (r#""1__280""#, 1280),
            (r#""1_280.0""#, 1280),
            (r#""1.28e0_3""#, 1280),
            (r#""+1280""#, 1280),
            (r#""-0""#, 0),
            (r#""18446744073709551615""#, u64::MAX),
        ] {
            assert_eq!(Json::parse(raw).unwrap().u64(), Ok(n));
        }
        for raw in [
            r#""_1""#,
            r#""1_""#,
            r#""-1""#,
            r#""++1""#,
            r#""18446744073709551616""#,
        ] {
            assert!(Json::parse(raw).unwrap().u64().is_err());
        }
    }
    #[test]
    fn escaped_and_drop_errors() {
        let j =
            Json::parse(r#"{"access_token":"a\uD83D\uDE00\nsecret","unknown":{"a":[1]}}"#).unwrap();
        assert_eq!(
            &**j.required("access_token").unwrap().string().unwrap(),
            "a😀\nsecret"
        );
        assert!(Json::parse(r#""secret\uD800x""#).is_err());
        assert!(
            Json::parse(r#"{"a":"x","a":"y"}"#)
                .unwrap()
                .field("a")
                .is_err()
        );
    }
}

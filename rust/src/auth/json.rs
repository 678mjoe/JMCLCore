//! Borrowed structural JSON; decoded text always belongs to a wiping buffer.
//! serde_json's RawValue/ignore_value path stores only container delimiters in
//! scratch, not decoded strings (audited 1.0.154). Never deserialize String/Value.
use zeroize::{Zeroize, Zeroizing};
#[derive(Clone, Copy)]
pub(crate) struct Json<'a>(pub &'a str);
impl<'a> Json<'a> {
    pub fn parse(s: &'a str) -> Result<Self, ()> {
        let _: &serde_json::value::RawValue = serde_json::from_str(s).map_err(|_| ())?;
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
        // Zig selects parseInt whenever no '.', 'e' or 'E' occurs. Its
        // integer syntax accepts a single sign and internal underscores.
        if !text.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
            let negative = text.starts_with('-');
            let digits = text
                .strip_prefix('+')
                .or_else(|| text.strip_prefix('-'))
                .unwrap_or(&text);
            if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') {
                return Err(());
            }
            let mut n = 0u64;
            for b in digits.bytes() {
                if b == b'_' {
                    continue;
                }
                if !b.is_ascii_digit() {
                    return Err(());
                }
                n = n
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u64::from(b - b'0')))
                    .ok_or(())?;
            }
            return if negative && n != 0 { Err(()) } else { Ok(n) };
        }
        let n = text.parse::<f64>().map_err(|_| ())?;
        if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n >= 18446744073709551616.0 {
            Err(())
        } else {
            Ok(n as u64)
        }
    }
    pub fn u32(self) -> Result<u32, ()> {
        self.u64()?.try_into().map_err(|_| ())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zig_integer_string_coercion() {
        for (raw, n) in [
            (r#""1_280""#, 1280),
            (r#""1__280""#, 1280),
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
        assert!(Json::parse(r#""secret\uD800x""#).unwrap().string().is_err());
        assert!(
            Json::parse(r#"{"a":"x","a":"y"}"#)
                .unwrap()
                .field("a")
                .is_err()
        );
    }
}

//! Dynamic RPC numbers retain Value/binary64 coercion. Persisted server records
//! instead retain the raw typed lexeme and use Zig-compatible binary128 parsing.
use serde::{Deserialize, Deserializer, de::Error};
use serde_json::Value;
fn float(v: &Value) -> Option<f64> {
    match v {
        Value::String(s)
            if !s.is_empty()
                && s.bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E')) =>
        {
            s.parse().ok()
        }
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}
pub fn u64_value(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        .or_else(|| {
            float(v)
                .filter(|v| v.is_finite() && v.fract() == 0. && *v >= 0. && *v < u64::MAX as f64)
                .map(|v| v as u64)
        })
}
pub fn u64_number<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    u64_value(&Value::deserialize(d)?).ok_or_else(|| D::Error::custom("InvalidNumber"))
}
fn persisted_unsigned<'de, D: Deserializer<'de>>(d: D, width: usize) -> Result<u64, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), width).map_err(|e| D::Error::custom(e.name()))
}
pub fn persisted_u64<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    persisted_unsigned(d, 64)
}
pub fn persisted_u16<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    persisted_unsigned(d, 16).map(|n| n as u16)
}
fn signed_text(text: &str) -> Result<i64, crate::json_number::NumberError> {
    use crate::json_number::NumberError;
    let negative = text.starts_with('-');
    let magnitude = text.strip_prefix(['+', '-']).unwrap_or(text);
    // Strip exactly one sign. Do not turn -+1/+-1/--1 into accepted input.
    if magnitude.starts_with(['+', '-']) {
        return Err(NumberError::InvalidCharacter);
    }
    let n = crate::json_number::unsigned_text(magnitude, 64)?;
    if negative {
        if n == 1u64 << 63 {
            Ok(i64::MIN)
        } else {
            i64::try_from(n)
                .map(|n| -n)
                .map_err(|_| NumberError::Overflow)
        }
    } else {
        i64::try_from(n).map_err(|_| NumberError::Overflow)
    }
}
pub fn persisted_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    use crate::json_number::NumberError;
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    let token = raw.get();
    let result = if token.starts_with('"') {
        crate::auth::json::Json(token)
            .string()
            .map_err(|_| NumberError::InvalidCharacter)
            .and_then(|text| signed_text(&text))
    } else if token
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
    {
        signed_text(token)
    } else {
        Err(NumberError::UnexpectedToken)
    };
    result.map_err(|e| D::Error::custom(e.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_persisted_signed_and_unsigned_widths() {
        #[derive(Deserialize)]
        struct Signed {
            #[serde(deserialize_with = "persisted_i64")]
            n: i64,
        }
        #[derive(Deserialize)]
        struct Wide {
            #[serde(deserialize_with = "persisted_u64")]
            n: u64,
        }
        #[derive(Deserialize)]
        struct Small {
            #[serde(deserialize_with = "persisted_u16")]
            n: u16,
        }
        let doc = |token: &str| format!(r#"{{"n":{token}}}"#);
        for (token, n) in [
            ("-9223372036854775808", i64::MIN),
            (r#""-9223372036854775808.0""#, i64::MIN),
            ("9223372036854775807", i64::MAX),
            (r#""9223372036854775807.0""#, i64::MAX),
            ("9007199254740993.0", 9007199254740993),
            (r#""-9007199254740993e0""#, -9007199254740993),
            (r#""-0""#, 0),
            ("-0.0", 0),
            (r#""+1_0""#, 10),
            (r#""-1__0""#, -10),
            (r#""0x1.8p+1""#, 3),
            (r#""-0x1.8p+1""#, -3),
            (r#""1e1""#, 10),
            (r#""3.00000000000000000000000000000000001""#, 3),
        ] {
            assert_eq!(
                serde_json::from_str::<Signed>(&doc(token)).unwrap().n,
                n,
                "{token}"
            )
        }
        for token in [
            "1.0000000000000001",
            r#""1.0000000000000001""#,
            r#""9223372036854775807.1""#,
            r#""9223372036854775808""#,
            r#""-9223372036854775809""#,
            r#""-+1""#,
            r#""+-1""#,
            r#""--1""#,
            r#""++1""#,
            r#"" 1""#,
            r#""1_""#,
            r#""0x1p0""#,
            r#""NaN""#,
            "null",
            "true",
            "[]",
        ] {
            assert!(
                serde_json::from_str::<Signed>(&doc(token)).is_err(),
                "{token}"
            );
        }
        for (token, n) in [
            ("18446744073709551615", u64::MAX),
            (r#""18446744073709551615.0""#, u64::MAX),
            (r#""0x1.8p+1""#, 3),
            (r#""+1__0""#, 10),
            ("-0.0", 0),
        ] {
            assert_eq!(serde_json::from_str::<Wide>(&doc(token)).unwrap().n, n);
        }
        for token in [
            "18446744073709551616",
            r#""18446744073709551615.1""#,
            r#""-1""#,
            "3.0000000000000001",
            r#""3.0000000000000001""#,
        ] {
            assert!(serde_json::from_str::<Wide>(&doc(token)).is_err())
        }
        for (token, n) in [
            ("65535", 65535),
            (r#""65535e0""#, 65535),
            (r#""0x1.5p4""#, 21),
        ] {
            assert_eq!(serde_json::from_str::<Small>(&doc(token)).unwrap().n, n)
        }
        for token in ["65536", r#""65535.0000000000000001""#] {
            assert!(serde_json::from_str::<Small>(&doc(token)).is_err())
        }
    }
    #[test]
    fn persisted_and_dynamic_deserializers_intentionally_differ() {
        #[derive(Deserialize)]
        struct Dynamic {
            #[serde(deserialize_with = "u64_number")]
            n: u64,
        }
        #[derive(Deserialize)]
        struct Persisted {
            #[serde(deserialize_with = "persisted_u64")]
            n: u64,
        }
        let raw = r#"{"n":3.0000000000000001}"#;
        assert_eq!(serde_json::from_str::<Dynamic>(raw).unwrap().n, 3);
        assert!(serde_json::from_str::<Persisted>(raw).is_err());
        let raw = r#"{"n":"3.00000000000000000000000000000000001"}"#;
        assert_eq!(serde_json::from_str::<Persisted>(raw).unwrap().n, 3);
        // No shared RPC grammar changes: dynamic Value and typed records are
        // separate, including their existing string conversion behavior.
        assert_eq!(u64_value(&serde_json::json!("3.0000000000000001")), Some(3));
    }
    #[test]
    fn zig_integer_strings_floats_and_boundaries() {
        for v in [
            serde_json::json!(10),
            serde_json::json!(10.0),
            serde_json::json!("1e1"),
            serde_json::json!("10.0"),
        ] {
            assert_eq!(u64_value(&v), Some(10));
        }
        assert_eq!(
            u64_value(&serde_json::json!(u64::MAX.to_string())),
            Some(u64::MAX)
        );
        for v in [
            serde_json::json!(-1),
            serde_json::json!(1.1),
            serde_json::json!(" 1"),
            serde_json::json!("18446744073709551616"),
            serde_json::Value::Null,
        ] {
            assert_eq!(u64_value(&v), None);
        }
    }
}

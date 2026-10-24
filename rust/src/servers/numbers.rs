//! Zig typed JSON integer compatibility for lifecycle/install records.
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
pub fn i64_number<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let v = Value::deserialize(d)?;
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        .or_else(|| {
            float(&v)
                .filter(|v| {
                    v.is_finite()
                        && v.fract() == 0.
                        && *v >= i64::MIN as f64
                        && *v < i64::MAX as f64
                })
                .map(|v| v as i64)
        })
        .ok_or_else(|| D::Error::custom("InvalidNumber"))
}
pub fn u16_number<'de, D: Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    u16::try_from(u64_number(d)?).map_err(D::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
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

//! Zig 0.16 typed unsigned-number coercion, retaining the source lexeme.
//! Typed schemas round decimal/hex floats to IEEE binary128 before checking
//! integrality. This is deliberately NOT decimal-is-integer or f64 coercion.
use rustc_apfloat::{Float, Round, Status, ieee::Quad};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberError {
    InvalidCharacter,
    InvalidNumber,
    Overflow,
    UnexpectedToken,
}
impl NumberError {
    pub fn name(self) -> &'static str {
        match self {
            Self::InvalidCharacter => "InvalidCharacter",
            Self::InvalidNumber => "InvalidNumber",
            Self::Overflow => "Overflow",
            Self::UnexpectedToken => "UnexpectedToken",
        }
    }
}

pub(crate) fn unsigned_text(text: &str, width: usize) -> Result<u64, NumberError> {
    use NumberError::*;
    let max = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    if !text.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
        let negative = text.starts_with('-');
        let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
        if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') {
            return Err(InvalidCharacter);
        }
        let mut n = 0u64;
        for b in digits.bytes() {
            if b == b'_' {
                continue;
            }
            if !b.is_ascii_digit() {
                return Err(InvalidCharacter);
            }
            if negative && b != b'0' {
                return Err(Overflow);
            }
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(b - b'0')))
                .filter(|n| *n <= max)
                .ok_or(Overflow)?;
        }
        return if negative && n != 0 {
            Err(Overflow)
        } else {
            Ok(n)
        };
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let hex = unsigned.starts_with("0x") || unsigned.starts_with("0X");
    let digit = |b: u8| {
        if hex {
            b.is_ascii_hexdigit()
        } else {
            b.is_ascii_digit()
        }
    };
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_'
            && (i == 0 || i + 1 == bytes.len() || !digit(bytes[i - 1]) || !digit(bytes[i + 1]))
        {
            return Err(InvalidCharacter);
        }
    }
    let mut clean = Zeroizing::new(text.replace('_', ""));
    // APFloat accepts missing decimal exponent digits for binutils compatibility;
    // Zig does not. Validate the grammar separately before conversion.
    let signless = clean.strip_prefix(['+', '-']).unwrap_or(&clean);
    let body = if hex { &signless[2..] } else { signless };
    let exp_chars = if hex { ['p', 'P'] } else { ['e', 'E'] };
    let mut parts = body.split(exp_chars);
    let mantissa = parts.next().unwrap_or("");
    let exponent = parts.next();
    if parts.next().is_some() {
        return Err(InvalidCharacter);
    }
    let mut digits = 0;
    let mut dots = 0;
    for b in mantissa.bytes() {
        if b == b'.' {
            dots += 1;
        } else if digit(b) {
            digits += 1;
        } else {
            return Err(InvalidCharacter);
        }
    }
    if digits == 0 || dots > 1 {
        return Err(InvalidCharacter);
    }
    if let Some(e) = exponent {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        if e.is_empty() || !e.bytes().all(|b| b.is_ascii_digit()) {
            return Err(InvalidCharacter);
        }
    } else if hex {
        // Zig accepts hex without p-exponent; APFloat requires one.
        clean.push_str("p0");
    }
    let n = Quad::from_str_r(&clean, Round::NearestTiesToEven)
        .map_err(|_| InvalidCharacter)?
        .value;
    if n.is_nan() {
        return Err(InvalidNumber);
    }
    if n.round_to_integral(Round::NearestTiesToAway).value != n {
        return Err(InvalidNumber);
    }
    if !n.is_finite() || (n.is_negative() && !n.is_zero()) {
        return Err(Overflow);
    }
    if n.is_zero() {
        return Ok(0);
    }
    let converted = n.to_u128_r(width, Round::TowardZero, &mut true);
    if converted.status.contains(Status::INVALID_OP) {
        return Err(Overflow);
    }
    Ok(converted.value as u64)
}

pub(crate) fn typed_raw(raw: &str, width: usize) -> Result<u64, NumberError> {
    if raw.starts_with('"') {
        let text = crate::auth::json::Json(raw)
            .string()
            .map_err(|_| NumberError::InvalidCharacter)?;
        unsigned_text(&text, width)
    } else if raw
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
    {
        unsigned_text(raw, width)
    } else {
        Err(NumberError::UnexpectedToken)
    }
}

/// Request params take Zig's dynamic Value path first. Finite JSON floats use
/// binary64; strings and non-finite/overflow number lexemes use typed parsing.
/// Source UB at the f64-rounded u64 upper bound is NOT reproduced (see evidence).
pub(crate) fn dynamic_raw(raw: &str, width: usize) -> Result<u64, NumberError> {
    if raw.starts_with('"') {
        return typed_raw(raw, width);
    }
    if raw == "-0" || raw.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
        let value = raw
            .parse::<f64>()
            .map_err(|_| NumberError::UnexpectedToken)?;
        if value.is_finite() {
            if value.fract() != 0.0 {
                return Err(NumberError::InvalidNumber);
            }
            let max = if width == 64 {
                u64::MAX
            } else {
                (1u64 << width) - 1
            };
            if value < 0.0 || value > max as f64 || value >= 18446744073709551616.0 {
                return Err(NumberError::Overflow);
            }
            return Ok(value as u64);
        }
    }
    typed_raw(raw, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_typed_unsigned_golden_corpus() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../compat/protocol-numbers.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let raw = case["raw"].as_str().unwrap();
            let width = case["width"].as_u64().unwrap() as usize;
            let actual = match typed_raw(raw, width) {
                Ok(value) => serde_json::json!({"value":value}),
                Err(error) => serde_json::json!({"error":error.name()}),
            };
            assert_eq!(actual, case["expected"], "u{width} {raw}");
        }
    }
    #[test]
    fn same_lexeme_has_distinct_typed_and_dynamic_rounding() {
        assert_eq!(
            typed_raw("1.0000000000000001", 32),
            Err(NumberError::InvalidNumber)
        );
        assert_eq!(dynamic_raw("1.0000000000000001", 32), Ok(1));
        assert_eq!(
            dynamic_raw(r#""1.0000000000000001""#, 32),
            Err(NumberError::InvalidNumber)
        );
        assert_eq!(typed_raw("18446744073709551615.0", 64), Ok(u64::MAX));
        // Deliberate non-abort: pinned Zig dynamic conversion has checked-cast UB.
        assert_eq!(
            dynamic_raw("18446744073709551615.0", 64),
            Err(NumberError::Overflow)
        );
    }
}

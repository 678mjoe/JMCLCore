//! Zig decimal unsigned CLI grammar, without any operational side effects.
pub(crate) fn unsigned(value: &str) -> Option<u64> {
    let negative = value.starts_with('-');
    let body = value.strip_prefix(['+', '-']).unwrap_or(value);
    if body.is_empty()
        || body.starts_with('_')
        || body.ends_with('_')
        || !body.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let n = body.replace('_', "").parse::<u64>().ok()?;
    if negative && n != 0 { None } else { Some(n) }
}

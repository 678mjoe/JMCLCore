//! Zig's []const u8 JSON accepts either a string or a byte array. Preserve the
//! Unicode subset in text fields without accidentally accepting tuple structs.
use serde_json::{Value, value::RawValue};
pub fn object(
    raw: &RawValue,
    fields: &[&str],
    optional: &[&str],
    lists: &[&str],
) -> Result<Value, ()> {
    let mut value: Value = serde_json::from_str(raw.get()).map_err(|_| ())?;
    let object = value.as_object_mut().ok_or(())?;
    for key in fields.iter().chain(optional) {
        if let Some(v) = object.get_mut(*key) {
            if v.is_null() && optional.contains(key) {
                continue;
            }
            text(v)?;
        }
    }
    for key in lists {
        if let Some(v) = object.get_mut(*key) {
            for item in v.as_array_mut().ok_or(())? {
                text(item)?;
            }
        }
    }
    Ok(value)
}
fn text(v: &mut Value) -> Result<(), ()> {
    if v.is_string() {
        return Ok(());
    }
    let bytes = v
        .as_array()
        .ok_or(())?
        .iter()
        .map(|v| {
            super::numbers::u64_value(v)
                .and_then(|n| u8::try_from(n).ok())
                .ok_or(())
        })
        .collect::<Result<Vec<_>, _>>()?;
    *v = String::from_utf8(bytes).map_err(|_| ())?.into();
    Ok(())
}

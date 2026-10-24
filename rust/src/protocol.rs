use crate::{NAME, PROTOCOL, VERSION};
use serde::{
    Deserialize, Serialize,
    de::{self, Visitor},
};
use serde_json::value::RawValue;
use std::{
    borrow::Cow,
    io::{self, Read, Write},
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}
impl RpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
const INVALID: &str = "Request must be valid JSON with protocol, id, and method fields";

// Field-level wiping also runs if deserialization fails after an escaped string
// was allocated but before a complete Request exists.
#[derive(Deserialize)]
#[serde(transparent)]
struct WipingText<'a>(#[serde(borrow)] Cow<'a, str>);
impl std::ops::Deref for WipingText<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.as_ref()
    }
}
impl AsRef<str> for WipingText<'_> {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl Drop for WipingText<'_> {
    fn drop(&mut self) {
        if let Cow::Owned(value) = &mut self.0 {
            value.zeroize();
        }
    }
}
#[derive(Deserialize)]
struct Request<'a> {
    #[serde(deserialize_with = "protocol_number")]
    protocol: u32,
    #[serde(borrow)]
    id: WipingText<'a>,
    #[serde(borrow)]
    method: WipingText<'a>,
    #[serde(borrow, default)]
    #[allow(dead_code)]
    params: Option<&'a RawValue>,
}

// Zig's typed JSON parser accepts integer-valued floats and decimal numeric strings.
fn protocol_number<'de, D: de::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    struct Protocol;
    impl<'de> Visitor<'de> for Protocol {
        type Value = u32;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a u32-compatible number")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u32, E> {
            u32::try_from(v).map_err(E::custom)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u32, E> {
            u32::try_from(v).map_err(E::custom)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u32, E> {
            if v.is_finite() && v >= 0.0 && v <= u32::MAX as f64 && v.fract() == 0.0 {
                Ok(v as u32)
            } else {
                Err(E::custom("not an integral u32"))
            }
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u32, E> {
            if v.is_empty()
                || v.bytes()
                    .any(|c| !matches!(c, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'))
            {
                return Err(E::custom("invalid number"));
            }
            self.visit_f64(v.parse::<f64>().map_err(E::custom)?)
        }
    }
    d.deserialize_any(Protocol)
}

#[derive(Serialize)]
struct Event<'a, T: Serialize> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    result: T,
}
#[derive(Serialize)]
struct SourceEvent<'a, T: Serialize> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    source: crate::metadata::Source,
    result: T,
}
#[derive(Serialize)]
struct Failure<'a> {
    protocol: u32,
    id: Option<&'a str>,
    event: &'static str,
    error: ErrorBody<'a>,
}
#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}
#[derive(Serialize)]
struct Version {
    name: &'static str,
    version: &'static str,
    protocol: u32,
}
#[derive(Serialize)]
struct Ping {
    pong: bool,
}

fn json_line(out: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")
}
pub fn error(out: &mut impl Write, id: Option<&str>, code: &str, message: &str) -> io::Result<()> {
    json_line(
        out,
        &Failure {
            protocol: PROTOCOL,
            id,
            event: "error",
            error: ErrorBody { code, message },
        },
    )
}
pub fn version_document(out: &mut impl Write) -> io::Result<()> {
    #[derive(Serialize)]
    struct Document {
        protocol: u32,
        name: &'static str,
        version: &'static str,
    }
    json_line(
        out,
        &Document {
            protocol: PROTOCOL,
            name: NAME,
            version: VERSION,
        },
    )
}
/// Check duplicate object keys in params without materializing secret values.
/// RawValue has already validated JSON syntax; this scanner is iterative so
/// adversarial nesting cannot overflow a recursive Rust visitor's stack.
fn params_unique(raw: &RawValue) -> bool {
    enum Frame {
        Array,
        Object {
            keys: Vec<Zeroizing<String>>,
            next_key: bool,
        },
    }
    let bytes = raw.get().as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => stack.push(Frame::Object {
                keys: Vec::new(),
                next_key: true,
            }),
            b'[' => stack.push(Frame::Array),
            b'}' | b']' => {
                stack.pop();
            }
            b',' => {
                if let Some(Frame::Object { next_key, .. }) = stack.last_mut() {
                    *next_key = true;
                }
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        break;
                    }
                    i += 1;
                }
                if let Some(Frame::Object { keys, next_key }) = stack.last_mut()
                    && *next_key
                {
                    let key: String = match serde_json::from_slice(&bytes[start..=i]) {
                        Ok(k) => k,
                        Err(_) => return false,
                    };
                    let key = Zeroizing::new(key);
                    if keys.iter().any(|existing| **existing == *key) {
                        return false;
                    }
                    keys.push(key);
                    *next_key = false;
                }
            }
            _ => {}
        }
        i += 1;
    }
    true
}

pub fn handle_line(line: &[u8], out: &mut impl Write) -> io::Result<()> {
    let request: Request<'_> = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(_) => return error(out, None, "INVALID_REQUEST", INVALID),
    };
    if request.params.is_some_and(|params| !params_unique(params)) {
        return error(out, None, "INVALID_REQUEST", INVALID);
    }
    if request.id.is_empty() {
        return error(out, None, "INVALID_REQUEST", "Request id must not be empty");
    }
    if request.protocol != PROTOCOL {
        return error(
            out,
            Some(&request.id),
            "UNSUPPORTED_PROTOCOL",
            "Unsupported protocol version",
        );
    }
    if let Some(result) = crate::version::handle(&request.method, request.params)
        .or_else(|| crate::java::select::handle(&request.method, request.params))
    {
        return match result {
            Ok(response) => json_line(
                out,
                &SourceEvent {
                    protocol: PROTOCOL,
                    id: &request.id,
                    event: "result",
                    source: response.source,
                    result: response.result,
                },
            ),
            Err(failure) => error(out, Some(&request.id), &failure.code, &failure.message),
        };
    }
    if let Some(result) = crate::accounts::handle(&request.method, request.params)
        .or_else(|| crate::instances::handle(&request.method, request.params))
        .or_else(|| crate::worlds::handle(&request.method, request.params))
        .or_else(|| crate::content::handle(&request.method, request.params))
        .or_else(|| crate::java::handle(&request.method, request.params))
    {
        return match result {
            Ok(result) => json_line(
                out,
                &Event {
                    protocol: PROTOCOL,
                    id: &request.id,
                    event: "result",
                    result,
                },
            ),
            Err(failure) => error(out, Some(&request.id), &failure.code, &failure.message),
        };
    }
    match request.method.as_ref() {
        "ping" => json_line(
            out,
            &Event {
                protocol: PROTOCOL,
                id: &request.id,
                event: "result",
                result: Ping { pong: true },
            },
        ),
        "core.version" => json_line(
            out,
            &Event {
                protocol: PROTOCOL,
                id: &request.id,
                event: "result",
                result: Version {
                    name: NAME,
                    version: VERSION,
                    protocol: PROTOCOL,
                },
            },
        ),
        method if known_method(method) => error(
            out,
            Some(&request.id),
            "NOT_IMPLEMENTED",
            "Rust rewrite method is not implemented; do not use as a replacement Core",
        ),
        _ => error(
            out,
            Some(&request.id),
            "METHOD_NOT_FOUND",
            "Unknown RPC method",
        ),
    }
}
fn known_method(method: &str) -> bool {
    // Generated pinned inventory is not an implementation or a compatibility claim.
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../compat/methods.json")).expect("checked inventory");
    inventory["methods"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(method))
}

/// Bounded storage, serial dispatch, CRLF and final unterminated line support.
/// The Zig oracle accepts at most 65535 bytes before newline; oversize requests
/// emit one INVALID_REQUEST and drain through the delimiter before recovery.
pub fn run(input: &mut impl Read, out: &mut impl Write) -> io::Result<()> {
    let mut line = Zeroizing::new(Vec::with_capacity(MAX_REQUEST_BYTES));
    let mut block = Zeroizing::new([0u8; 4096]);
    let mut oversized = false;
    loop {
        let n = input.read(&mut *block)?;
        if n == 0 {
            if oversized {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EndOfStream"));
            } else if !line.is_empty() {
                dispatch(&line, out)?;
            }
            out.flush()?;
            return Ok(());
        }
        for &b in &block[..n] {
            if b == b'\n' {
                if oversized {
                    error(out, None, "INVALID_REQUEST", INVALID)?;
                } else {
                    dispatch(&line, out)?;
                }
                out.flush()?;
                line.zeroize();
                line.clear();
                oversized = false;
            } else if !oversized {
                if line.len() == MAX_REQUEST_BYTES - 1 {
                    line.zeroize();
                    line.clear();
                    oversized = true;
                } else {
                    line.push(b);
                }
            }
        }
        block[..n].zeroize();
    }
}
fn dispatch(line: &[u8], out: &mut impl Write) -> io::Result<()> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() {
        Ok(())
    } else {
        handle_line(line, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_recovers_and_params_remain_uninterpreted() {
        let input=b"bad\n{\"protocol\":1,\"id\":\"a\",\"method\":\"ping\",\"params\":{\"secret\":\"never-output\"}}\n";
        let mut out = Vec::new();
        run(&mut &input[..], &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("INVALID_REQUEST"));
        assert!(text.contains("pong"));
        assert!(!text.contains("never-output"));
    }
    #[test]
    fn bounded_line_recovers() {
        let mut input = vec![b'x'; MAX_REQUEST_BYTES * 2];
        input.extend_from_slice(b"\n{\"protocol\":1,\"id\":\"x\",\"method\":\"ping\"}");
        let mut out = Vec::new();
        run(&mut &input[..], &mut out).unwrap();
        assert_eq!(out.iter().filter(|b| **b == b'\n').count(), 2);
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    #[test]
    fn escaped_duplicate_keys_rejected_before_dispatch() {
        let line = br#"{"protocol":1,"id":"x","method":"ping","params":{"key":1,"\u006bey":2}}"#;
        let mut out = Vec::new();
        handle_line(line, &mut out).unwrap();
        let event: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(event["id"].is_null());
        assert_eq!(event["error"]["code"], "INVALID_REQUEST");
    }
    #[test]
    fn duplicate_nested_objects_and_keys_inside_string_values_are_distinct() {
        let line=br#"{"protocol":1,"id":"x","method":"ping","params":{"a":{"x":1},"b":{"x":2},"value":"{\"x\":1,\"x\":2}"}}"#;
        let mut out = Vec::new();
        handle_line(line, &mut out).unwrap();
        let event: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(event["result"]["pong"], true);
    }
    #[test]
    fn deep_params_are_checked_without_recursive_visitor() {
        let mut line = br#"{"protocol":1,"id":"x","method":"ping","params":"#.to_vec();
        line.extend(std::iter::repeat_n(b'[', 1024));
        line.extend_from_slice(b"null");
        line.extend(std::iter::repeat_n(b']', 1024));
        line.push(b'}');
        let mut out = Vec::new();
        handle_line(&line, &mut out).unwrap();
        let event: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(event["result"]["pong"], true);
    }
}

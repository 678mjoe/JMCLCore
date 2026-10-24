//! Java properties codec matching the pinned Zig Core; no server lifecycle actions.
use std::fmt;

pub const MAX_FILE_BYTES: usize = 256 * 1024;
pub const MAX_ENTRIES: usize = 1024;
pub const MAX_KEY_BYTES: usize = 256;
pub const MAX_VALUE_BYTES: usize = 64 * 1024;
pub const DEFAULT_CONTENT: &str = "#Minecraft server properties\ndifficulty=easy\nenable-rcon=false\nenable-query=false\ngamemode=survival\nlevel-name=world\nmax-players=20\nmotd=A Minecraft Server\nonline-mode=true\nserver-ip=\nserver-port=25565\nquery.port=25565\nrcon.port=25575\nrcon.password=\nview-distance=10\n";
pub const EULA_CONTENT: &str = "# By changing the setting below to TRUE you indicate your agreement to the Minecraft EULA (https://aka.ms/MinecraftEULA).\neula=true\n";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    pub key: String,
    pub value: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub entries: Vec<Entry>,
}
impl Document {
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), Error> {
        if key.is_empty()
            || key.len() > MAX_KEY_BYTES
            || key.bytes().any(|c| matches!(c, 0 | b'\r' | b'\n'))
        {
            return Err(Error("InvalidServerPropertyKey"));
        }
        if value.len() > MAX_VALUE_BYTES || value.contains('\0') {
            return Err(Error("InvalidServerPropertyValue"));
        }
        if let Some(e) = self.entries.iter_mut().find(|e| e.key == key) {
            e.value = value.into();
            return Ok(());
        }
        if self.entries.len() == MAX_ENTRIES {
            return Err(Error("TooManyServerProperties"));
        }
        self.entries.push(Entry {
            key: key.into(),
            value: value.into(),
        });
        Ok(())
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::from(&b"#Minecraft server properties\n"[..]);
        for e in &self.entries {
            escape(&mut out, e.key.as_bytes(), true);
            out.push(b'=');
            escape(&mut out, e.value.as_bytes(), false);
            out.push(b'\n');
        }
        out
    }
    pub fn rcon_port(&self) -> Result<Option<u16>, Error> {
        if !self.enabled("enable-rcon")? {
            return Ok(None);
        }
        Ok(Some(port(self.get("rcon.port"), 25575)?))
    }
    pub fn query_port(&self) -> Result<Option<u16>, Error> {
        if !self.enabled("enable-query")? {
            return Ok(None);
        }
        Ok(Some(port(
            self.get("query.port").or(self.get("server-port")),
            25565,
        )?))
    }
    fn enabled(&self, key: &str) -> Result<bool, Error> {
        match self.get(key) {
            None => Ok(false),
            Some(s) if s.eq_ignore_ascii_case("true") => Ok(true),
            Some(s) if s.eq_ignore_ascii_case("false") => Ok(false),
            _ => Err(Error("InvalidServerPropertyBoolean")),
        }
    }
}
fn port(raw: Option<&str>, default: u16) -> Result<u16, Error> {
    let Some(raw) = raw else { return Ok(default) };
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error("InvalidServerPropertyPort"));
    }
    raw.parse::<u16>()
        .ok()
        .filter(|v| *v != 0)
        .ok_or(Error("InvalidServerPropertyPort"))
}
fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 12)
}
pub fn parse(bytes: &[u8]) -> Result<Document, Error> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error("ServerPropertiesTooLarge"));
    }
    let mut d = Document::default();
    let mut logical = Vec::new();
    let mut continuing = false;
    for raw in bytes.split(|b| *b == b'\n') {
        let mut part = raw.strip_suffix(b"\r").unwrap_or(raw);
        if continuing {
            part = part.trim_ascii_start_matches();
        }
        logical.extend_from_slice(part);
        if logical.iter().rev().take_while(|b| **b == b'\\').count() % 2 == 1 {
            logical.pop();
            continuing = true;
            continue;
        }
        continuing = false;
        parse_line(&mut d, &logical)?;
        logical.clear();
    }
    if continuing {
        parse_line(&mut d, &logical)?;
    }
    Ok(d)
}
trait PropertyTrim {
    fn trim_ascii_start_matches(&self) -> &[u8];
}
impl PropertyTrim for [u8] {
    fn trim_ascii_start_matches(&self) -> &[u8] {
        let n = self.iter().take_while(|b| ws(**b)).count();
        &self[n..]
    }
}
fn parse_line(d: &mut Document, line: &[u8]) -> Result<(), Error> {
    let mut i = 0;
    while i < line.len() && ws(line[i]) {
        i += 1;
    }
    if i == line.len() || matches!(line[i], b'#' | b'!') {
        return Ok(());
    }
    let start = i;
    let mut escaped = false;
    while i < line.len() {
        let b = line[i];
        if !escaped && (matches!(b, b'=' | b':') || ws(b)) {
            break;
        }
        escaped = b == b'\\' && !escaped;
        i += 1;
    }
    let end = i;
    while i < line.len() && ws(line[i]) {
        i += 1;
    }
    if i < line.len() && matches!(line[i], b'=' | b':') {
        i += 1;
    }
    while i < line.len() && ws(line[i]) {
        i += 1;
    }
    let key = decode(&line[start..end])?;
    let value = decode(&line[i..])?;
    let key = std::str::from_utf8(&key).map_err(|_| Error("InvalidServerPropertyKey"))?;
    let value = std::str::from_utf8(&value).map_err(|_| Error("InvalidServerPropertyValue"))?;
    d.set(key, value)
}
fn hex(raw: &[u8], i: &mut usize) -> Result<u16, Error> {
    if raw.len().saturating_sub(*i) < 4 {
        return Err(Error("InvalidServerProperties"));
    }
    let mut n = 0u16;
    for b in &raw[*i..*i + 4] {
        n = n * 16
            + (*b as char)
                .to_digit(16)
                .ok_or(Error("InvalidServerProperties"))? as u16;
    }
    *i += 4;
    Ok(n)
}
fn decode(raw: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        i += 1;
        if b != b'\\' {
            out.push(b);
            continue;
        }
        if i == raw.len() {
            out.push(b'\\');
            break;
        }
        let b = raw[i];
        i += 1;
        match b {
            b't' => out.push(b'\t'),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b'f' => out.push(12),
            b'u' => {
                let first = hex(raw, &mut i)?;
                let cp = if (0xd800..=0xdbff).contains(&first) {
                    if raw.get(i..i + 2) != Some(b"\\u") {
                        return Err(Error("InvalidServerProperties"));
                    }
                    i += 2;
                    let second = hex(raw, &mut i)?;
                    if !(0xdc00..=0xdfff).contains(&second) {
                        return Err(Error("InvalidServerProperties"));
                    }
                    0x10000 + ((first as u32 - 0xd800) << 10) + (second as u32 - 0xdc00)
                } else {
                    first as u32
                };
                let c = char::from_u32(cp).ok_or(Error("InvalidServerProperties"))?;
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            _ => out.push(b),
        }
    }
    Ok(out)
}
fn escape(out: &mut Vec<u8>, raw: &[u8], key: bool) {
    for (i, b) in raw.iter().copied().enumerate() {
        match b {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            12 => out.extend_from_slice(b"\\f"),
            b' ' if key || i == 0 => out.extend_from_slice(b"\\ "),
            b'=' | b':' if key => {
                out.push(b'\\');
                out.push(b);
            }
            b'#' | b'!' if key || i == 0 => {
                out.push(b'\\');
                out.push(b);
            }
            _ => out.push(b),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes_continuations_duplicates() {
        let d=parse(b"  # ignored\nescaped\\ key=value\\ with\\ spaces\ncontinued=first\\\n  second\nunicode=\\u4f60\\u597d\\uD83D\\uDE00\nduplicate=first\nduplicate:second\n").unwrap();
        assert_eq!(d.entries.len(), 4);
        assert_eq!(d.get("escaped key"), Some("value with spaces"));
        assert_eq!(d.get("continued"), Some("firstsecond"));
        assert_eq!(d.get("unicode"), Some("你好😀"));
        assert_eq!(d.get("duplicate"), Some("second"));
        assert_eq!(parse(&d.encode()).unwrap(), d);
    }
    #[test]
    fn invalid_unicode_utf8_and_limits() {
        assert_eq!(
            parse(b"motd=\\uD83Dbroken\n").unwrap_err().0,
            "InvalidServerProperties"
        );
        assert_eq!(
            parse(b"motd=\xff\n").unwrap_err().0,
            "InvalidServerPropertyValue"
        );
        assert!(parse(&vec![b' '; MAX_FILE_BYTES + 1]).is_err());
    }
    #[test]
    fn defaults_ports() {
        let mut d = parse(DEFAULT_CONTENT.as_bytes()).unwrap();
        assert_eq!(d.rcon_port().unwrap(), None);
        d.set("enable-rcon", "TrUe").unwrap();
        assert_eq!(d.rcon_port().unwrap(), Some(25575));
        d.set("rcon.port", " 1").unwrap();
        assert!(d.rcon_port().is_err());
        d.set("enable-query", "true").unwrap();
        assert_eq!(d.query_port().unwrap(), Some(25565));
    }
}

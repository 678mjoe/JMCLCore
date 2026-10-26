//! Ordered persisted manifest schema; public serde/construction APIs stay separate.
use super::DisplayName;
use crate::content::modrinth::project_ordered::{Cursor, Token, decode};
use crate::content::modrinth::{Error, Result};
mod sealed {
    pub trait Sealed {}
}
pub(crate) trait Name {
    fn read(c: &mut Cursor<'_>) -> Result<Self>
    where
        Self: Sized;
}
impl Name for String {
    fn read(c: &mut Cursor<'_>) -> Result<Self> {
        if c.peek_value()? != b'"' {
            return Err("UnexpectedToken".into());
        }
        let Token::String(raw) = c.next()? else {
            return c.error();
        };
        String::from_utf8(decode(raw)?).map_err(|_| "SyntaxError".into())
    }
}
impl Name for DisplayName {
    fn read(c: &mut Cursor<'_>) -> Result<Self> {
        Ok(Self(c.value_bytes()?.into_bytes()))
    }
}
pub(crate) struct Wire<N> {
    schema_version: u32,
    id: String,
    name: N,
    version_id: String,
    fabric_loader: Option<String>,
    neoforge_version: Option<String>,
    forge_version: Option<String>,
    source: String,
    accept_eula: bool,
    java_path: Option<String>,
}
pub(crate) trait Document: sealed::Sealed {
    type Name: Name;
    const SERVER: bool;
    fn from_wire(w: Wire<Self::Name>) -> Self;
}
macro_rules! document {
    ($ty:ty, $name:ty, $server:expr $(, $extra:ident)*) => {
        impl sealed::Sealed for $ty {}
        impl Document for $ty {
            type Name = $name;
            const SERVER: bool = $server;
            fn from_wire(w: Wire<$name>) -> Self {
                Self { schema_version:w.schema_version, id:w.id, name:w.name,
                    version_id:w.version_id, fabric_loader:w.fabric_loader,
                    neoforge_version:w.neoforge_version, forge_version:w.forge_version,
                    source:w.source, $($extra:w.$extra,)* }
            }
        }
    };
}
document!(super::Manifest, String, false);
document!(super::ReadManifest, DisplayName, false);
document!(
    crate::servers::manifest::Manifest,
    String,
    true,
    accept_eula,
    java_path
);
#[cfg(target_os = "linux")]
document!(
    crate::servers::manifest::ReadManifest,
    DisplayName,
    true,
    accept_eula,
    java_path
);
fn optional(c: &mut Cursor<'_>) -> Result<Option<String>> {
    if c.peek_value()? == b'n' {
        c.next()?;
        Ok(None)
    } else {
        String::read(c).map(Some)
    }
}
fn source(c: &mut Cursor<'_>) -> Result<String> {
    c.peek_value()?;
    let value = match c.next()? {
        Token::String(raw) => {
            String::from_utf8(decode(raw)?).map_err(|_| Error("SyntaxError".into()))?
        }
        Token::Number(raw) => raw.to_owned(),
        _ => return Err("UnexpectedToken".into()),
    };
    super::source_tag(&value)
        .map(str::to_owned)
        .ok_or_else(|| "InvalidEnumTag".into())
}
fn number(c: &mut Cursor<'_>) -> Result<u32> {
    c.peek_value()?;
    let raw = match c.next()? {
        Token::String(raw) | Token::Number(raw) => raw,
        _ => return Err("UnexpectedToken".into()),
    };
    crate::json_number::typed_raw(raw, 32)
        .map(|n| n as u32)
        .map_err(|e| e.name().into())
}
fn wire<N: Name>(bytes: &[u8], server: bool) -> Result<Wire<N>> {
    let mut c = Cursor::new(bytes);
    c.peek_value()?;
    if c.next()? != Token::OpenObject {
        return Err("UnexpectedToken".into());
    }
    let names: &[&[u8]] = &[
        b"schema_version",
        b"id",
        b"name",
        b"version_id",
        b"fabric_loader",
        b"neoforge_version",
        b"forge_version",
        b"source",
        b"accept_eula",
        b"java_path",
    ];
    let (mut id, mut name, mut version_id) = (None, None, None);
    let (mut fabric_loader, mut neoforge_version, mut forge_version, mut java_path) =
        (None, None, None, None);
    let (mut schema_version, mut source_value, mut accept_eula) = (1, "official".to_owned(), false);
    let mut seen = 0u16;
    let mut key = c.object_key(true)?;
    if key != Token::CloseObject {
        loop {
            let Token::String(raw) = key else {
                return c.error();
            };
            let field = decode(raw)?;
            let index = names[..if server { 10 } else { 8 }]
                .iter()
                .position(|n| *n == field.as_slice());
            if let Some(i) = index {
                let bit = 1u16 << i;
                if seen & bit != 0 {
                    return Err("DuplicateField".into());
                }
                seen |= bit;
            }
            c.expect(Token::Colon)?;
            match index {
                Some(0) => schema_version = number(&mut c)?,
                Some(1) => id = Some(String::read(&mut c)?),
                Some(2) => name = Some(N::read(&mut c)?),
                Some(3) => version_id = Some(String::read(&mut c)?),
                Some(4) => fabric_loader = optional(&mut c)?,
                Some(5) => neoforge_version = optional(&mut c)?,
                Some(6) => forge_version = optional(&mut c)?,
                Some(7) => source_value = source(&mut c)?,
                Some(8) => {
                    c.peek_value()?;
                    let Token::Bool(v) = c.next()? else {
                        return Err("UnexpectedToken".into());
                    };
                    accept_eula = v;
                }
                Some(9) => java_path = optional(&mut c)?,
                _ => c.skip()?,
            }
            match c.separator(Token::CloseObject, false)? {
                Token::CloseObject => break,
                Token::Comma => key = c.object_key(false)?,
                _ => return c.error(),
            }
        }
    }
    let w = Wire {
        schema_version,
        id: id.ok_or_else(|| Error("MissingField".into()))?,
        name: name.ok_or_else(|| Error("MissingField".into()))?,
        version_id: version_id.ok_or_else(|| Error("MissingField".into()))?,
        fabric_loader,
        neoforge_version,
        forge_version,
        source: source_value,
        accept_eula,
        java_path,
    };
    c.finish()?;
    Ok(w)
}
pub(super) fn parse<T: Document>(bytes: &[u8]) -> std::result::Result<T, &'static str> {
    wire::<T::Name>(bytes, T::SERVER)
        .map(T::from_wire)
        .map_err(|e| match e.0.as_str() {
            "UnexpectedEndOfInput" => "UnexpectedEndOfInput",
            "UnexpectedToken" => "UnexpectedToken",
            "InvalidEnumTag" => "InvalidEnumTag",
            "Overflow" => "Overflow",
            "InvalidNumber" => "InvalidNumber",
            "InvalidCharacter" => "InvalidCharacter",
            "MissingField" => "MissingField",
            "DuplicateField" => "DuplicateField",
            _ => "SyntaxError",
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn all(bytes: &[u8]) -> Vec<std::result::Result<(), &'static str>> {
        #[allow(unused_mut)]
        let mut results = vec![
            parse::<crate::instances::Manifest>(bytes).map(|_| ()),
            parse::<crate::instances::ReadManifest>(bytes).map(|_| ()),
            parse::<crate::servers::manifest::Manifest>(bytes).map(|_| ()),
        ];
        #[cfg(target_os = "linux")]
        results.push(parse::<crate::servers::manifest::ReadManifest>(bytes).map(|_| ()));
        results
    }
    #[test]
    fn typed_errors_precede_later_scanner_errors_but_not_earlier_ones() {
        for (bytes, error) in [
            (br#"{"source":"bad","#.as_slice(), "InvalidEnumTag"),
            (br#"{"source":"bad",!"#, "InvalidEnumTag"),
            (br#"{"source":"bad","future":"\ud800"}"#, "InvalidEnumTag"),
            (br#"{"future":"\ud800","source":"bad"}"#, "SyntaxError"),
            (
                br#"{"future":{"a":"\ud800"},"source":"bad"}"#,
                "SyntaxError",
            ),
            (br#"{"schema_version":4294967296,"#, "Overflow"),
            (br#"{"id":true,"#, "UnexpectedToken"),
            (br#"{"name":"A","name":"B","#, "DuplicateField"),
            (br#"{"source":"official","#, "UnexpectedEndOfInput"),
            (br#"{}!"#, "MissingField"),
            (
                br#"{"id":"demo","name":"","version_id":"fake"}!"#,
                "SyntaxError",
            ),
            (br#"[]"#, "UnexpectedToken"),
        ] {
            for result in all(bytes) {
                assert_eq!(result.unwrap_err(), error, "{bytes:?}");
            }
        }
    }
    #[test]
    fn complete_fields_defaults_and_name_boundaries_do_not_widen() {
        let bytes=br#"{"id":"demo","name":"Demo","version_id":"fake","future":{"nested":[1e400]},"source":"0_1","fabric_loader":null,"neoforge_version":"n","accept_eula":true,"java_path":"java"}"#;
        for result in all(bytes) {
            assert!(result.is_ok());
        }
        let server = parse::<crate::servers::manifest::Manifest>(bytes).unwrap();
        assert_eq!(server.schema_version, 1);
        assert_eq!(server.source, "bmclapi");
        assert!(server.accept_eula);
        assert_eq!(server.java_path.as_deref(), Some("java"));
        assert_eq!(server.neoforge_version.as_deref(), Some("n"));
        assert!(server.fabric_loader.is_none());
        let raw = br#"{"id":"demo","name":[255],"version_id":"fake"}"#;
        assert_eq!(
            parse::<crate::instances::Manifest>(raw).unwrap_err(),
            "UnexpectedToken"
        );
        assert_eq!(
            parse::<crate::servers::manifest::Manifest>(raw).unwrap_err(),
            "UnexpectedToken"
        );
        assert_eq!(
            parse::<crate::instances::ReadManifest>(raw).unwrap().name.0,
            vec![255]
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            parse::<crate::servers::manifest::ReadManifest>(raw)
                .unwrap()
                .name
                .0,
            vec![255]
        );
        // Server-only keys remain ignored (including duplicates) for instances.
        let special=br#"{"id":"demo","name":"Demo","version_id":"fake","accept_eula":true,"accept_eula":false}"#;
        assert!(parse::<crate::instances::Manifest>(special).is_ok());
        assert_eq!(
            parse::<crate::servers::manifest::Manifest>(special).unwrap_err(),
            "DuplicateField"
        );
    }
}

#[cfg(test)]
mod size_tests {
    #[test]
    fn existing_four_manifest_size_guards_precede_schema_parsing() {
        let mut bytes = br#"{"id":"demo","name":"Demo","version_id":"fake"}"#.to_vec();
        bytes.resize(65536, b' ');
        assert!(crate::instances::Manifest::parse(&bytes, "demo").is_ok());
        assert!(crate::instances::ReadManifest::parse(&bytes, "demo").is_ok());
        assert!(crate::servers::manifest::Manifest::parse(&bytes, "demo").is_ok());
        #[cfg(target_os = "linux")]
        assert!(crate::servers::manifest::ReadManifest::parse(&bytes, "demo").is_ok());
        bytes.push(b' ');
        assert_eq!(
            crate::instances::Manifest::parse(&bytes, "demo").unwrap_err(),
            "InstanceManifestTooLarge"
        );
        assert_eq!(
            crate::instances::ReadManifest::parse(&bytes, "demo").unwrap_err(),
            "InstanceManifestTooLarge"
        );
        assert_eq!(
            crate::servers::manifest::Manifest::parse(&bytes, "demo").unwrap_err(),
            "ServerManifestTooLarge"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            crate::servers::manifest::ReadManifest::parse(&bytes, "demo").unwrap_err(),
            "ServerManifestTooLarge"
        );
    }
}

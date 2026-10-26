//! Private ordered persisted registry schema. Public serde contracts are unchanged.
use super::{Kind, Provider, RegistryFile, WireEntry, WireRegistry, WireSource};
use crate::content::modrinth::project_ordered::{Cursor, Token, decode};
use crate::content::modrinth::{Error, MetadataBytes, Result};
fn object(
    c: &mut Cursor<'_>,
    names: &[&[u8]],
    mut field: impl FnMut(usize, &mut Cursor<'_>) -> Result<()>,
) -> Result<()> {
    c.peek_value()?;
    match c.next()? {
        Token::OpenObject => {}
        Token::End => return Err("UnexpectedEndOfInput".into()),
        _ => return Err("UnexpectedToken".into()),
    }
    let mut seen = 0u16;
    let mut key = c.object_key(true)?;
    if key == Token::CloseObject {
        return Ok(());
    }
    loop {
        let Token::String(raw) = key else {
            return c.error();
        };
        let name = decode(raw)?;
        let index = names.iter().position(|n| *n == name.as_slice());
        if let Some(i) = index {
            let bit = 1u16 << i;
            if seen & bit != 0 {
                return Err("DuplicateField".into());
            }
            seen |= bit;
        }
        c.expect(Token::Colon)?;
        if let Some(i) = index {
            field(i, c)?
        } else {
            c.skip()?
        }
        match c.separator(Token::CloseObject, false)? {
            Token::CloseObject => return Ok(()),
            Token::Comma => key = c.object_key(false)?,
            _ => return c.error(),
        }
    }
}
fn list<T>(
    c: &mut Cursor<'_>,
    child_peeks: bool,
    mut item: impl FnMut(&mut Cursor<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let mut out = vec![];
    if !c.array_start()? {
        return Ok(out);
    }
    loop {
        out.push(item(c)?);
        match c.separator(Token::CloseArray, child_peeks)? {
            Token::CloseArray => return Ok(out),
            Token::Comma => {}
            _ => return c.error(),
        }
    }
}

fn optional(c: &mut Cursor<'_>) -> Result<Option<MetadataBytes>> {
    if c.peek_value()? == b'n' {
        c.next()?;
        Ok(None)
    } else {
        c.value_bytes().map(Some)
    }
}
fn number(c: &mut Cursor<'_>, width: usize) -> Result<u64> {
    c.peek_value()?;
    match c.next()? {
        Token::String(raw) | Token::Number(raw) => {
            crate::json_number::typed_raw(raw, width).map_err(|e| Error(e.name().into()))
        }
        Token::End => Err("UnexpectedEndOfInput".into()),
        _ => Err("UnexpectedToken".into()),
    }
}
fn boolean(c: &mut Cursor<'_>) -> Result<bool> {
    c.peek_value()?;
    match c.next()? {
        Token::Bool(v) => Ok(v),
        Token::End => Err("UnexpectedEndOfInput".into()),
        _ => Err("UnexpectedToken".into()),
    }
}
fn enumeration(c: &mut Cursor<'_>, names: &[&str]) -> Result<usize> {
    c.peek_value()?;
    let text = match c.next()? {
        Token::String(raw) => {
            String::from_utf8(decode(raw)?).map_err(|_| Error("InvalidEnumTag".into()))?
        }
        Token::Number(raw) => raw.to_owned(),
        Token::End => return Err("UnexpectedEndOfInput".into()),
        _ => return Err("UnexpectedToken".into()),
    };
    super::wire_enum_index(&text, names).map_err(Error::from)
}
fn source(c: &mut Cursor<'_>) -> Result<WireSource> {
    let (mut provider, mut project_id, mut version_id, mut slug) = (None, None, None, None);
    object(
        c,
        &[b"provider", b"project_id", b"version_id", b"slug"],
        |i, c| {
            match i {
                0 => {
                    provider = Some(
                        [Provider::Modrinth, Provider::Curseforge, Provider::Manual]
                            [enumeration(c, &["modrinth", "curseforge", "manual"])?],
                    )
                }
                1 => project_id = optional(c)?,
                2 => version_id = optional(c)?,
                3 => slug = optional(c)?,
                _ => unreachable!(),
            };
            Ok(())
        },
    )?;
    Ok(WireSource {
        provider: provider.ok_or("MissingField")?,
        project_id,
        version_id,
        slug,
    })
}
pub(super) trait WireFile: RegistryFile {
    fn from_wire(bytes: MetadataBytes) -> Result<Self>;
}
impl WireFile for String {
    fn from_wire(bytes: MetadataBytes) -> Result<Self> {
        String::from_utf8(bytes.into_bytes()).map_err(|_| Error("UnexpectedToken".into()))
    }
}
impl WireFile for crate::content::ListedPath {
    fn from_wire(bytes: MetadataBytes) -> Result<Self> {
        Ok(Self(bytes.into_bytes()))
    }
}
fn file<F: WireFile>(c: &mut Cursor<'_>) -> Result<F> {
    F::from_wire(c.value_bytes()?)
}
fn entry<F: WireFile>(c: &mut Cursor<'_>) -> Result<WireEntry<F>> {
    let (mut kind, mut file_value, mut sha1, mut size, mut src) = (None, None, None, None, None);
    let (mut enabled, mut as_dependency) = (true, false);
    object(
        c,
        &[
            b"kind",
            b"file",
            b"sha1",
            b"size",
            b"source",
            b"enabled",
            b"as_dependency",
        ],
        |i, c| {
            match i {
                0 => {
                    kind = Some(
                        [
                            Kind::Mod,
                            Kind::Resourcepack,
                            Kind::Shaderpack,
                            Kind::Datapack,
                        ][enumeration(c, &["mod", "resourcepack", "shaderpack", "datapack"])?],
                    )
                }
                1 => file_value = Some(file(c)?),
                2 => sha1 = Some(c.value_bytes()?),
                3 => size = Some(number(c, 64)?),
                4 => src = Some(source(c)?),
                5 => enabled = boolean(c)?,
                6 => as_dependency = boolean(c)?,
                _ => unreachable!(),
            };
            Ok(())
        },
    )?;
    Ok(WireEntry {
        kind: kind.ok_or("MissingField")?,
        file: file_value.ok_or("MissingField")?,
        sha1: sha1.ok_or("MissingField")?,
        size: size.ok_or("MissingField")?,
        source: src.ok_or("MissingField")?,
        enabled,
        as_dependency,
    })
}
fn document<F: WireFile>(bytes: &[u8]) -> Result<WireRegistry<F>> {
    let mut c = Cursor::new(bytes);
    let (mut schema_version, mut entries) = (1, vec![]);
    object(&mut c, &[b"schema_version", b"entries"], |i, c| {
        match i {
            0 => schema_version = number(c, 32)? as u32,
            1 => entries = list(c, false, entry)?,
            _ => unreachable!(),
        };
        Ok(())
    })?;
    c.finish()?;
    Ok(WireRegistry {
        schema_version,
        entries,
    })
}
pub(super) fn parse<F: WireFile>(
    bytes: &[u8],
) -> std::result::Result<WireRegistry<F>, &'static str> {
    document(bytes).map_err(|e| match e.0.as_str() {
        "SyntaxError" => "SyntaxError",
        "UnexpectedToken" => "UnexpectedToken",
        "UnexpectedEndOfInput" => "UnexpectedEndOfInput",
        "DuplicateField" => "DuplicateField",
        "MissingField" => "MissingField",
        "InvalidEnumTag" => "InvalidEnumTag",
        "InvalidNumber" => "InvalidNumber",
        "InvalidCharacter" => "InvalidCharacter",
        "Overflow" => "Overflow",
        _ => unreachable!("private cursor/schema emitted an unlisted error: {}", e.0),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn registry_file_views_preserve_conversion_boundary() {
        let mut b = format!(
            "{{\"entries\":[{{\"kind\":\"mod\",\"file\":[255],\"sha1\":\"{}\",\"size\":1,\"source\":{{\"provider\":\"manual\"}}}}]}}",
            "a".repeat(40)
        );
        assert_eq!(
            super::super::parse(b.as_bytes()).unwrap_err(),
            "UnexpectedToken"
        );
        assert_eq!(
            super::super::parse_listing(b.as_bytes()).unwrap_err(),
            "ContentKindPathMismatch"
        );
        b = b.replace("[255]", "[109,111,100,115,47,255,46,106,97,114]");
        assert_eq!(
            super::super::parse(b.as_bytes()).unwrap_err(),
            "UnexpectedToken"
        );
        let r = super::super::parse_listing(b.as_bytes()).unwrap();
        assert_eq!(r.entries[0].file.0, b"mods/\xff.jar");
    }
}

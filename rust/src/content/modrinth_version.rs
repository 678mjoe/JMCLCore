//! Private ordered typed Version schemas. Shared provider semantics run later.
use super::project_ordered::{Cursor, Token, decode};
use super::wire::{WireDependency, WireFile, WireHashes, WireVersion};
use super::{DependencyType, Error, FilenameValue, MetadataBytes, Result};

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
fn boolean(c: &mut Cursor<'_>) -> Result<bool> {
    c.peek_value()?;
    match c.next()? {
        Token::Bool(v) => Ok(v),
        Token::End => Err("UnexpectedEndOfInput".into()),
        _ => Err("UnexpectedToken".into()),
    }
}
fn number(c: &mut Cursor<'_>) -> Result<u64> {
    c.peek_value()?;
    match c.next()? {
        Token::String(raw) | Token::Number(raw) => {
            crate::json_number::typed_raw(raw, 64).map_err(|e| Error(e.name().into()))
        }
        Token::End => Err("UnexpectedEndOfInput".into()),
        _ => Err("UnexpectedToken".into()),
    }
}
fn dependency_type(c: &mut Cursor<'_>) -> Result<DependencyType> {
    c.peek_value()?;
    let bytes = match c.next()? {
        Token::String(raw) => decode(raw)?,
        Token::Number(raw) => raw.as_bytes().to_vec(),
        Token::End => return Err("UnexpectedEndOfInput".into()),
        _ => return Err("UnexpectedToken".into()),
    };
    match bytes.as_slice() {
        b"required" => Ok(DependencyType::Required),
        b"optional" => Ok(DependencyType::Optional),
        b"incompatible" => Ok(DependencyType::Incompatible),
        b"embedded" => Ok(DependencyType::Embedded),
        _ => {
            let text = std::str::from_utf8(&bytes).map_err(|_| Error("InvalidEnumTag".into()))?;
            // Zig sliceToEnum accepts integer spelling only, not float coercion.
            if text.starts_with('-') || text.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
                return Err("InvalidEnumTag".into());
            }
            match crate::json_number::unsigned_text(text, 2)
                .map_err(|_| Error("InvalidEnumTag".into()))?
            {
                0 => Ok(DependencyType::Required),
                1 => Ok(DependencyType::Optional),
                2 => Ok(DependencyType::Incompatible),
                3 => Ok(DependencyType::Embedded),
                _ => Err("InvalidEnumTag".into()),
            }
        }
    }
}
fn hashes(c: &mut Cursor<'_>) -> Result<WireHashes> {
    let (mut sha1, mut sha512) = (None, None);
    object(c, &[b"sha1", b"sha512"], |i, c| {
        match i {
            0 => sha1 = Some(c.value_bytes()?),
            1 => sha512 = Some(optional(c)?),
            _ => unreachable!(),
        }
        Ok(())
    })?;
    Ok(WireHashes {
        sha1: sha1.ok_or("MissingField")?,
        sha512: sha512.unwrap_or(None),
    })
}
fn file<F: FilenameValue>(c: &mut Cursor<'_>) -> Result<WireFile<F>> {
    let (mut url, mut filename, mut primary, mut hash, mut size) = (None, None, None, None, None);
    object(
        c,
        &[b"url", b"filename", b"primary", b"hashes", b"size"],
        |i, c| {
            match i {
                0 => {
                    url = Some(
                        String::from_utf8(c.value_bytes()?.into_bytes())
                            .map_err(|_| Error("UnexpectedToken".into()))?,
                    )
                }
                1 => filename = Some(F::from_bytes(c.value_bytes()?)?),
                2 => primary = Some(boolean(c)?),
                3 => hash = Some(hashes(c)?),
                4 => size = Some(number(c)?),
                _ => unreachable!(),
            }
            Ok(())
        },
    )?;
    Ok(WireFile {
        url: url.ok_or("MissingField")?,
        filename: filename.ok_or("MissingField")?,
        primary: primary.unwrap_or(false),
        hashes: hash.ok_or("MissingField")?,
        size: size.ok_or("MissingField")?,
    })
}
fn dependency(c: &mut Cursor<'_>) -> Result<WireDependency> {
    let (mut version_id, mut project_id, mut file_name, mut kind) = (None, None, None, None);
    object(
        c,
        &[
            b"version_id",
            b"project_id",
            b"file_name",
            b"dependency_type",
        ],
        |i, c| {
            match i {
                0 => version_id = Some(optional(c)?),
                1 => project_id = Some(optional(c)?),
                2 => file_name = Some(optional(c)?),
                3 => kind = Some(dependency_type(c)?),
                _ => unreachable!(),
            }
            Ok(())
        },
    )?;
    Ok(WireDependency {
        version_id: version_id.unwrap_or(None),
        project_id: project_id.unwrap_or(None),
        file_name: file_name.unwrap_or(None),
        dependency_type: kind.ok_or("MissingField")?,
    })
}
fn version<F: FilenameValue>(c: &mut Cursor<'_>) -> Result<WireVersion<F>> {
    let (
        mut id,
        mut project_id,
        mut version_number,
        mut version_type,
        mut game_versions,
        mut loaders,
        mut files,
        mut dependencies,
    ) = (None, None, None, None, None, None, None, None);
    object(
        c,
        &[
            b"id",
            b"project_id",
            b"version_number",
            b"version_type",
            b"game_versions",
            b"loaders",
            b"files",
            b"dependencies",
        ],
        |i, c| {
            match i {
                0 => id = Some(c.value_bytes()?),
                1 => project_id = Some(c.value_bytes()?),
                2 => version_number = Some(c.value_bytes()?),
                3 => version_type = Some(c.value_bytes()?),
                4 => game_versions = Some(list(c, true, |c| c.value_bytes())?),
                5 => loaders = Some(list(c, true, |c| c.value_bytes())?),
                6 => files = Some(list(c, false, file)?),
                7 => dependencies = Some(list(c, false, dependency)?),
                _ => unreachable!(),
            }
            Ok(())
        },
    )?;
    Ok(WireVersion {
        id: id.ok_or("MissingField")?,
        project_id: project_id.ok_or("MissingField")?,
        version_number: version_number.ok_or("MissingField")?,
        version_type: version_type.ok_or("MissingField")?,
        game_versions: game_versions.ok_or("MissingField")?,
        loaders: loaders.ok_or("MissingField")?,
        files: files.ok_or("MissingField")?,
        dependencies: dependencies.unwrap_or_default(),
    })
}
pub(super) fn one<F: FilenameValue>(bytes: &[u8]) -> Result<WireVersion<F>> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("ModrinthResponseTooLarge".into());
    }
    let mut c = Cursor::new(bytes);
    let value = version(&mut c)?;
    c.finish()?;
    Ok(value)
}
pub(super) fn many<F: FilenameValue>(bytes: &[u8]) -> Result<Vec<WireVersion<F>>> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("ModrinthResponseTooLarge".into());
    }
    let mut c = Cursor::new(bytes);
    let values = list(&mut c, false, version)?;
    c.finish()?;
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> serde_json::Value {
        serde_json::json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"url":"https://fixture.invalid/a.jar","filename":"a.jar","hashes":{"sha1":"a".repeat(40)},"size":7}]})
    }
    #[test]
    fn filename_views_keep_distinct_typed_boundaries() {
        let mut v = fixture();
        v["files"][0]["filename"] = serde_json::json!([97, 255, 46, 106, 97, 114]);
        let b = serde_json::to_vec(&v).unwrap();
        assert!(matches!(one::<String>(&b),Err(Error(e)) if e=="UnexpectedToken"));
        let typed = one::<MetadataBytes>(&b).unwrap();
        assert_eq!(typed.files[0].filename.as_bytes(), b"a\xff.jar");
        super::super::validate(&typed).unwrap();
        v["files"][0].as_object_mut().unwrap().remove("size");
        let b = serde_json::to_vec(&v).unwrap();
        assert!(matches!(one::<String>(&b),Err(Error(e)) if e=="UnexpectedToken"));
        assert!(matches!(one::<MetadataBytes>(&b),Err(Error(e)) if e=="MissingField"));
    }
    #[test]
    fn opaque_url_boundary_is_not_widened() {
        let mut v = fixture();
        let mut url = b"https://fixture.invalid/".to_vec();
        url.push(255);
        v["files"][0]["url"] = serde_json::json!(url);
        let b = serde_json::to_vec(&v).unwrap();
        assert!(matches!(one::<String>(&b),Err(Error(e)) if e=="UnexpectedToken"));
        assert!(matches!(one::<MetadataBytes>(&b),Err(Error(e)) if e=="UnexpectedToken"));
    }
}

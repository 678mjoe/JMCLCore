//! Ordered File and Page envelopes; Project and download-URL parsing are separate.
use super::{Dependency, Detail, Hash, Pagination, mr};
use mr::project_ordered::{Cursor, Token, decode};
use mr::{Error, FilenameValue, Result};

pub(super) fn object(
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

pub(super) fn number(c: &mut Cursor<'_>, width: usize) -> Result<u64> {
    c.peek_value()?;
    match c.next()? {
        Token::String(raw) | Token::Number(raw) => {
            crate::json_number::typed_raw(raw, width).map_err(|e| Error(e.name().into()))
        }
        Token::End => Err("UnexpectedEndOfInput".into()),
        _ => Err("UnexpectedToken".into()),
    }
}
pub(super) fn boolean(c: &mut Cursor<'_>) -> Result<bool> {
    c.peek_value()?;
    match c.next()? {
        Token::Bool(v) => Ok(v),
        Token::End => Err("UnexpectedEndOfInput".into()),
        _ => Err("UnexpectedToken".into()),
    }
}
fn optional_string(c: &mut Cursor<'_>) -> Result<Option<String>> {
    if c.peek_value()? == b'n' {
        c.next()?;
        Ok(None)
    } else {
        String::from_utf8(c.value_bytes()?.into_bytes())
            .map(Some)
            .map_err(|_| Error("UnexpectedToken".into()))
    }
}
fn hash(c: &mut Cursor<'_>) -> Result<Hash> {
    let (mut value, mut algo) = (None, None);
    object(c, &[b"value", b"algo"], |i, c| {
        match i {
            0 => value = Some(c.value_bytes()?),
            1 => algo = Some(number(c, 8)? as u8),
            _ => unreachable!(),
        };
        Ok(())
    })?;
    Ok(Hash {
        value: value.ok_or("MissingField")?,
        algo: algo.ok_or("MissingField")?,
    })
}
fn dependency(c: &mut Cursor<'_>) -> Result<Dependency> {
    let (mut id, mut relation) = (None, None);
    object(c, &[b"modId", b"relationType"], |i, c| {
        match i {
            0 => id = Some(number(c, 32)? as u32),
            1 => relation = Some(number(c, 8)? as u8),
            _ => unreachable!(),
        };
        Ok(())
    })?;
    Ok(Dependency {
        id: id.ok_or("MissingField")?,
        relation: relation.ok_or("MissingField")?,
    })
}
fn pagination(c: &mut Cursor<'_>) -> Result<Option<Pagination>> {
    if c.peek_value()? == b'n' {
        c.next()?;
        return Ok(None);
    }
    let mut p = Pagination {
        index: 0,
        page_size: 0,
        result_count: 0,
        total: 0,
    };
    object(
        c,
        &[b"index", b"pageSize", b"resultCount", b"totalCount"],
        |i, c| {
            let n = number(c, 32)? as u32;
            match i {
                0 => p.index = n,
                1 => p.page_size = n,
                2 => p.result_count = n,
                3 => p.total = n,
                _ => unreachable!(),
            };
            Ok(())
        },
    )?;
    Ok(Some(p))
}
fn detail<F: FilenameValue>(c: &mut Cursor<'_>) -> Result<Detail<F>> {
    let (
        mut id,
        mut project,
        mut available,
        mut name,
        mut release,
        mut hashes,
        mut date,
        mut size,
        mut url,
        mut games,
        mut dependencies,
    ) = (
        None, None, None, None, None, None, None, None, None, None, None,
    );
    object(
        c,
        &[
            b"id",
            b"modId",
            b"isAvailable",
            b"fileName",
            b"releaseType",
            b"hashes",
            b"fileDate",
            b"fileLength",
            b"downloadUrl",
            b"gameVersions",
            b"dependencies",
        ],
        |i, c| {
            match i {
                0 => id = Some(number(c, 32)? as u32),
                1 => project = Some(number(c, 32)? as u32),
                2 => available = Some(boolean(c)?),
                3 => name = Some(F::from_bytes(c.value_bytes()?)?),
                4 => release = Some(number(c, 8)? as u8),
                5 => hashes = Some(list(c, false, hash)?),
                6 => date = Some(c.value_bytes()?),
                7 => size = Some(number(c, 64)?),
                8 => url = Some(optional_string(c)?),
                9 => games = Some(list(c, true, |c| c.value_bytes())?),
                10 => dependencies = Some(list(c, false, dependency)?),
                _ => unreachable!(),
            };
            Ok(())
        },
    )?;
    Ok(Detail {
        id: id.ok_or("MissingField")?,
        project: project.ok_or("MissingField")?,
        available: available.unwrap_or(true),
        name: name.ok_or("MissingField")?,
        release: release.ok_or("MissingField")?,
        hashes: hashes.ok_or("MissingField")?,
        date: date.ok_or("MissingField")?,
        size: size.ok_or("MissingField")?,
        url: url.unwrap_or(None),
        games: games.ok_or("MissingField")?,
        dependencies: dependencies.unwrap_or_default(),
    })
}
// Complete envelope typing precedes cardinality and semantic validation.
pub(super) fn envelope<T>(
    bytes: &[u8],
    limit: usize,
    mut read_data: impl FnMut(&mut Cursor<'_>) -> Result<T>,
) -> Result<(T, Option<Pagination>)> {
    if bytes.len() > limit {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let mut c = Cursor::new(bytes);
    let (mut data, mut page) = (None, None);
    object(&mut c, &[b"data", b"pagination"], |i, c| {
        match i {
            0 => data = Some(read_data(c)?),
            1 => page = pagination(c)?,
            _ => unreachable!(),
        };
        Ok(())
    })?;
    let data = data.ok_or("MissingField")?;
    c.finish()?;
    Ok((data, page))
}
pub(super) fn parse<F: FilenameValue>(bytes: &[u8]) -> Result<Detail<F>> {
    envelope(bytes, 2 * 1024 * 1024, detail).map(|(data, _)| data)
}
pub(super) fn page<F: FilenameValue>(bytes: &[u8]) -> Result<(Vec<Detail<F>>, Option<Pagination>)> {
    envelope(bytes, 8 * 1024 * 1024, |c| list(c, false, detail))
}

pub(super) fn url(bytes: &[u8]) -> Result<Option<String>> {
    envelope(bytes, 2 * 1024 * 1024, optional_string).map(|(data, _)| data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filename_and_url_representation_boundaries_are_preserved() {
        let v = serde_json::json!({"data":{"id":1,"modId":42,"fileName":[97,255,46,106,97,114],"releaseType":1,"hashes":[{"algo":1,"value":"a".repeat(40)}],"fileDate":"2024","fileLength":7,"gameVersions":["1.20.1"]}});
        let b = serde_json::to_vec(&v).unwrap();
        assert!(matches!(parse::<String>(&b),Err(Error(e)) if e=="UnexpectedToken"));
        assert_eq!(
            parse::<mr::MetadataBytes>(&b).unwrap().name.as_bytes(),
            b"a\xff.jar"
        );
        let mut missing = v.clone();
        missing["data"]
            .as_object_mut()
            .unwrap()
            .remove("fileLength");
        let b = serde_json::to_vec(&missing).unwrap();
        assert!(matches!(parse::<String>(&b),Err(Error(e)) if e=="UnexpectedToken"));
        assert!(matches!(parse::<mr::MetadataBytes>(&b),Err(Error(e)) if e=="MissingField"));
        let mut url = v;
        url["data"]["downloadUrl"] = serde_json::json!([104, 116, 116, 112, 115, 58, 47, 47, 255]);
        let b = serde_json::to_vec(&url).unwrap();
        assert!(matches!(parse::<mr::MetadataBytes>(&b),Err(Error(e)) if e=="UnexpectedToken"));
    }
}

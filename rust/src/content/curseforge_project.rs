//! Ordered Project envelope; operational slug conversion follows validation.
use super::{
    file_ordered::{boolean, envelope, number, object},
    mr,
};
use mr::project_ordered::Cursor;
use mr::{MetadataBytes, Result};
pub(super) struct ParsedProject {
    pub id: u32,
    pub slug: MetadataBytes,
    pub name: MetadataBytes,
    pub class_id: u32,
    pub available: bool,
}
fn project(c: &mut Cursor<'_>) -> Result<ParsedProject> {
    let (mut id, mut slug, mut name, mut class_id, mut available) = (None, None, None, None, None);
    object(
        c,
        &[b"id", b"slug", b"name", b"classId", b"isAvailable"],
        |i, c| {
            match i {
                0 => id = Some(number(c, 32)? as u32),
                1 => slug = Some(c.value_bytes()?),
                2 => name = Some(c.value_bytes()?),
                3 => class_id = Some(number(c, 32)? as u32),
                4 => available = Some(boolean(c)?),
                _ => unreachable!(),
            };
            Ok(())
        },
    )?;
    Ok(ParsedProject {
        id: id.ok_or("MissingField")?,
        slug: slug.ok_or("MissingField")?,
        name: name.ok_or("MissingField")?,
        class_id: class_id.ok_or("MissingField")?,
        available: available.unwrap_or(true),
    })
}
pub(super) fn parse(bytes: &[u8]) -> Result<ParsedProject> {
    envelope(bytes, 2 * 1024 * 1024, project).map(|(data, _)| data)
}

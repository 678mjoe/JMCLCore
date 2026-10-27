//! Conditional managed-install view over one bounded manifest buffer.
use super::*;
use crate::content::modrinth::project_ordered::{Cursor, Token, decode};
pub(crate) enum Selection {
    Vanilla {
        version: Vec<u8>,
        source: crate::metadata::Source,
    },
    Legacy(Manifest),
}
// Classification has no diagnostics. Indeterminate syntax retains the original
// parser; this does not claim complete opaque malformed-document parity.
fn vanilla(bytes: &[u8]) -> Option<bool> {
    let mut c = Cursor::new(bytes);
    c.peek_value().ok()?;
    if c.next().ok()? != Token::OpenObject {
        return None;
    }
    let mut key = c.object_key(true).ok()?;
    if key != Token::CloseObject {
        loop {
            let Token::String(raw) = key else { return None };
            let field = decode(raw).ok()?;
            c.expect(Token::Colon).ok()?;
            if [
                b"fabric_loader".as_slice(),
                b"neoforge_version",
                b"forge_version",
            ]
            .contains(&field.as_slice())
                && c.peek_value().ok()? != b'n'
            {
                return Some(false);
            }
            c.skip().ok()?;
            match c.separator(Token::CloseObject, false).ok()? {
                Token::CloseObject => break,
                Token::Comma => key = c.object_key(false).ok()?,
                _ => return None,
            }
        }
    }
    c.finish().ok()?;
    Some(true)
}
#[cfg(test)]
thread_local! {static PARSE_CALLS:std::cell::Cell<usize>=const {std::cell::Cell::new(0)};}
#[cfg(test)]
pub(crate) fn reset_count() {
    PARSE_CALLS.set(0)
}
#[cfg(test)]
pub(crate) fn parse_count() -> usize {
    PARSE_CALLS.get()
}
pub(super) fn parse(bytes: &[u8], id: &str) -> Result<Selection, &'static str> {
    #[cfg(test)]
    PARSE_CALLS.set(PARSE_CALLS.get() + 1);
    if vanilla(bytes) == Some(true) {
        let m = super::byte_read::Manifest::parse(bytes, id)?;
        Ok(Selection::Vanilla {
            version: m.version_id.0,
            source: crate::metadata::Source::parse(&m.source).expect("validated manifest source"),
        })
    } else {
        Manifest::parse(bytes, id).map(Selection::Legacy)
    }
}
#[cfg(test)]
mod tests;

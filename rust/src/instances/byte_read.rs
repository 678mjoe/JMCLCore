//! Read-only instance presentation and receipt identity. Operational APIs stay String-based.
use super::manifest_ordered::{Name, number, source};
use super::{DisplayName, MAX_MANIFEST_BYTES, validate_id};
use crate::content::modrinth::project_ordered::{Cursor, Token, decode};
use crate::content::modrinth::{Error, Result};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
pub(super) struct Manifest {
    schema_version: u32,
    id: DisplayName,
    pub(super) name: DisplayName,
    pub(super) version_id: DisplayName,
    fabric_loader: Option<DisplayName>,
    neoforge_version: Option<DisplayName>,
    forge_version: Option<DisplayName>,
    pub(super) source: String,
}
struct Wire {
    schema: u32,
    id: Option<DisplayName>,
    name: Option<DisplayName>,
    version: Option<DisplayName>,
    fabric: Option<DisplayName>,
    neo: Option<DisplayName>,
    forge: Option<DisplayName>,
    source: String,
    os: Option<DisplayName>,
    arch: Option<DisplayName>,
}
fn optional(c: &mut Cursor<'_>) -> Result<Option<DisplayName>> {
    if c.peek_value()? == b'n' {
        c.next()?;
        Ok(None)
    } else {
        DisplayName::read(c).map(Some)
    }
}
// Source-ordered typed parsing: duplicate/error order is not a serde Value map.
// Unknown fields are skipped; only the chosen persisted schema is recognized.
fn wire(bytes: &[u8], receipt: bool) -> Result<Wire> {
    let mut c = Cursor::new(bytes);
    c.peek_value()?;
    if c.next()? != Token::OpenObject {
        return Err("UnexpectedToken".into());
    }
    let mut w = Wire {
        schema: 1,
        id: None,
        name: None,
        version: None,
        fabric: None,
        neo: None,
        forge: None,
        source: "official".into(),
        os: None,
        arch: None,
    };
    let names: &[&[u8]] = &[
        b"schema_version",
        b"id",
        b"name",
        b"version_id",
        b"fabric_loader",
        b"neoforge_version",
        b"forge_version",
        b"source",
        b"os",
        b"arch",
    ];
    let mut seen = 0u16;
    let mut key = c.object_key(true)?;
    if key != Token::CloseObject {
        loop {
            let Token::String(raw) = key else {
                return c.error();
            };
            let field = decode(raw)?;
            let index = names
                .iter()
                .position(|name| *name == field.as_slice())
                .filter(|i| if receipt { *i != 1 && *i != 2 } else { *i < 8 });
            if let Some(i) = index {
                let bit = 1u16 << i;
                if seen & bit != 0 {
                    return Err("DuplicateField".into());
                }
                seen |= bit;
            }
            c.expect(Token::Colon)?;
            match index {
                Some(0) => w.schema = number(&mut c)?,
                Some(1) => w.id = Some(DisplayName::read(&mut c)?),
                Some(2) => w.name = Some(DisplayName::read(&mut c)?),
                Some(3) => w.version = Some(DisplayName::read(&mut c)?),
                Some(4) => w.fabric = optional(&mut c)?,
                Some(5) => w.neo = optional(&mut c)?,
                Some(6) => w.forge = optional(&mut c)?,
                Some(7) => w.source = source(&mut c)?,
                Some(8) => w.os = Some(DisplayName::read(&mut c)?),
                Some(9) => w.arch = Some(DisplayName::read(&mut c)?),
                _ => c.skip()?,
            }
            match c.separator(Token::CloseObject, false)? {
                Token::CloseObject => break,
                Token::Comma => key = c.object_key(false)?,
                _ => return c.error(),
            }
        }
    }
    if w.version.is_none()
        || if receipt {
            w.os.is_none() || w.arch.is_none()
        } else {
            w.id.is_none() || w.name.is_none()
        }
    {
        return Err(Error("MissingField".into()));
    }
    c.finish()?;
    Ok(w)
}
fn category(error: Error) -> &'static str {
    match error.0.as_str() {
        "UnexpectedEndOfInput" => "UnexpectedEndOfInput",
        "UnexpectedToken" => "UnexpectedToken",
        "InvalidEnumTag" => "InvalidEnumTag",
        "Overflow" => "Overflow",
        "InvalidNumber" => "InvalidNumber",
        "InvalidCharacter" => "InvalidCharacter",
        "MissingField" => "MissingField",
        "DuplicateField" => "DuplicateField",
        _ => "SyntaxError",
    }
}
fn length(bytes: &DisplayName, max: usize) -> bool {
    !bytes.0.is_empty() && bytes.0.len() <= max
}
fn loaders<'a>(
    fabric: &'a Option<DisplayName>,
    neo: &'a Option<DisplayName>,
    forge: &'a Option<DisplayName>,
) -> [&'a Option<DisplayName>; 3] {
    [fabric, neo, forge]
}
impl Manifest {
    pub(super) fn parse(bytes: &[u8], id: &str) -> std::result::Result<Self, &'static str> {
        validate_id(id)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err("InstanceManifestTooLarge");
        }
        let w = wire(bytes, false).map_err(category)?;
        let m = Self {
            schema_version: w.schema,
            id: w.id.unwrap(),
            name: w.name.unwrap(),
            version_id: w.version.unwrap(),
            fabric_loader: w.fabric,
            neoforge_version: w.neo,
            forge_version: w.forge,
            source: w.source,
        };
        let text = std::str::from_utf8(&m.id.0).map_err(|_| "InvalidInstanceId")?;
        validate_id(text)?;
        if m.schema_version != 1 {
            return Err("UnsupportedInstanceSchema");
        }
        if !length(&m.name, 256) {
            return Err("InvalidInstanceName");
        }
        if !length(&m.version_id, 256) {
            return Err("InvalidVersionId");
        }
        for (v, error) in [
            (m.fabric_loader.as_ref(), "InvalidFabricLoaderVersion"),
            (m.neoforge_version.as_ref(), "InvalidNeoForgeVersion"),
            (m.forge_version.as_ref(), "InvalidForgeVersion"),
        ] {
            if v.is_some_and(|v| !length(v, 256)) {
                return Err(error);
            }
        }
        if loaders(&m.fabric_loader, &m.neoforge_version, &m.forge_version)
            .iter()
            .filter(|v| v.is_some())
            .count()
            > 1
        {
            return Err("LoaderConflict");
        }
        if m.id.0 != id.as_bytes() {
            return Err("InstanceIdMismatch");
        }
        Ok(m)
    }
    pub(super) fn id(&self) -> &str {
        std::str::from_utf8(&self.id.0).expect("validated ASCII instance ID")
    }
    pub(super) fn fields(&self, installed: bool) -> Value {
        json!({"id":self.id,"name":self.name,"version_id":self.version_id,"fabric_loader":self.fabric_loader,"neoforge_version":self.neoforge_version,"forge_version":self.forge_version,"source":self.source,"installed":installed})
    }
    pub(super) fn matches_receipt(&self, bytes: &[u8]) -> bool {
        if bytes.len() > 16 * 1024 {
            return false;
        }
        let Ok(w) = wire(bytes, true) else {
            return false;
        };
        if w.schema != 1
            || !length(w.version.as_ref().unwrap(), 256)
            || !length(w.os.as_ref().unwrap(), 32)
            || !length(w.arch.as_ref().unwrap(), 32)
        {
            return false;
        }
        let values = loaders(&w.fabric, &w.neo, &w.forge);
        if values.iter().filter(|v| v.is_some()).count() > 1
            || values
                .iter()
                .filter_map(|v| v.as_ref())
                .any(|v| !length(v, 256))
        {
            return false;
        }
        // Zig compares only version + optional loader bytes after validating receipt.
        w.version.as_ref() == Some(&self.version_id)
            && w.fabric == self.fabric_loader
            && w.neo == self.neoforge_version
            && w.forge == self.forge_version
    }
}

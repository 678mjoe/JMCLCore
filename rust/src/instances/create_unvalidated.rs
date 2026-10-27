//! Explicit validate:false creation only. No resolver or operational read model.
use super::{LookupId, RpcError, invalid, validate_id};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Debug)]
struct Bytes(Vec<u8>);
impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        LookupId::deserialize(d).map(|v| Self(v.0))
    }
}
impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => s.serialize_str(text),
            Err(_) => self.0.serialize(s),
        }
    }
}
fn official() -> Bytes {
    Bytes(b"official".to_vec())
}
#[derive(Deserialize)]
struct Input {
    directory: Bytes,
    id: Bytes,
    version_id: Bytes,
    #[serde(default)]
    name: Option<Bytes>,
    #[serde(default)]
    fabric_loader: Option<Bytes>,
    #[serde(default)]
    neoforge_version: Option<Bytes>,
    #[serde(default)]
    forge_version: Option<Bytes>,
    #[serde(default = "official")]
    source: Bytes,
    validate: bool,
}

// Selection is deliberately narrow: omitted, null, malformed and true values
// continue through the existing String-based decoder and resolver path.
pub(super) fn selected(raw: Option<&RawValue>) -> bool {
    #[derive(Deserialize)]
    struct Gate<'a> {
        #[serde(borrow)]
        validate: &'a RawValue,
    }
    raw.filter(|raw| raw.get().trim_start().starts_with('{'))
        .and_then(|raw| serde_json::from_str::<Gate<'_>>(raw.get()).ok())
        .is_some_and(|gate| gate.validate.get() == "false")
}

pub(super) struct Prepared {
    directory: Bytes,
    manifest: ByteManifest,
}
#[derive(Serialize)]
struct ByteManifest {
    schema_version: u32,
    id: String,
    name: Bytes,
    version_id: Bytes,
    fabric_loader: Option<Bytes>,
    neoforge_version: Option<Bytes>,
    forge_version: Option<Bytes>,
    source: String,
}

pub(super) fn prepare(raw: Option<&RawValue>) -> Result<Prepared, RpcError> {
    if !selected(raw) {
        return Err(invalid("instance creation parameters are invalid"));
    }
    let input: Input = super::parse(
        raw,
        "instance creation requires directory, id, and version_id",
        "instance creation parameters are invalid",
    )?;
    if input.validate {
        return Err(invalid("instance creation parameters are invalid"));
    }
    // Preserve protocol/instance.zig checks before root opening and late manifest
    // validation. No UTF-8 requirement applies to display/version/loader bytes.
    if input.directory.0.is_empty() || input.id.0.is_empty() || input.version_id.0.is_empty() {
        return Err(invalid("instance creation parameters are out of range"));
    }
    let id = std::str::from_utf8(&input.id.0)
        .ok()
        .filter(|id| validate_id(id).is_ok())
        .ok_or_else(|| invalid("the instance id is invalid"))?
        .to_owned();
    let source = match input.source.0.as_slice() {
        b"official" => "official",
        b"bmclapi" => "bmclapi",
        _ => return Err(invalid("source must be official or bmclapi")),
    };
    if [
        &input.fabric_loader,
        &input.neoforge_version,
        &input.forge_version,
    ]
    .iter()
    .filter(|v| v.is_some())
    .count()
        > 1
    {
        return Err(invalid("only one loader field may be set"));
    }
    Ok(Prepared {
        directory: input.directory,
        manifest: ByteManifest {
            schema_version: 1,
            name: input.name.unwrap_or_else(|| Bytes(id.as_bytes().to_vec())),
            id,
            version_id: input.version_id,
            fabric_loader: input.fabric_loader,
            neoforge_version: input.neoforge_version,
            forge_version: input.forge_version,
            source: source.into(),
        },
    })
}
impl ByteManifest {
    fn validate(&self) -> Result<(), &'static str> {
        validate_id(&self.id)?;
        for (bytes, error) in [
            (Some(&self.name), "InvalidInstanceName"),
            (Some(&self.version_id), "InvalidVersionId"),
            (self.fabric_loader.as_ref(), "InvalidFabricLoaderVersion"),
            (self.neoforge_version.as_ref(), "InvalidNeoForgeVersion"),
            (self.forge_version.as_ref(), "InvalidForgeVersion"),
        ] {
            if let Some(bytes) = bytes
                && (bytes.0.is_empty() || bytes.0.len() > 256)
            {
                return Err(error);
            }
        }
        Ok(())
    }
    fn encode(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self).map_err(|_| "InvalidInstanceManifest")?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

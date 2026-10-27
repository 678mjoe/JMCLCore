//! Low-level receipt codec/writer. Calling it does not verify installation completion.
use super::Error;
use crate::{metadata::Source, plan::Target, store::ArtifactRoot};
use serde::Serialize;

pub(crate) struct Identity<'a> {
    pub version: &'a [u8],
    pub fabric: Option<&'a [u8]>,
    pub neo: Option<&'a [u8]>,
    pub forge: Option<&'a [u8]>,
    pub source: Source,
}
#[derive(Clone, Copy)]
struct Bytes<'a>(&'a [u8]);
impl Serialize for Bytes<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(self.0) {
            Ok(text) => s.serialize_str(text),
            Err(_) => self.0.serialize(s),
        }
    }
}
pub(crate) fn encode(p: &Identity<'_>, target: &Target) -> Result<Vec<u8>, Error> {
    if p.version.is_empty() || p.version.len() > 256 {
        return Err(Error::named("InvalidVersionId"));
    }
    let loaders = [p.fabric, p.neo, p.forge];
    for v in loaders.iter().flatten() {
        if v.is_empty() || v.len() > 256 {
            return Err(Error::named("InvalidLoaderVersion"));
        }
    }
    if loaders.iter().filter(|v| v.is_some()).count() > 1 {
        return Err(Error::named("LoaderConflict"));
    }
    if target.os.is_empty()
        || target.os.len() > 32
        || target.arch.is_empty()
        || target.arch.len() > 32
    {
        return Err(Error::named("InvalidInstallationTarget"));
    }
    #[derive(Serialize)]
    struct Receipt<'a> {
        schema_version: u32,
        version_id: Bytes<'a>,
        fabric_loader: Option<Bytes<'a>>,
        neoforge_version: Option<Bytes<'a>>,
        forge_version: Option<Bytes<'a>>,
        source: Source,
        os: &'a str,
        arch: &'a str,
    }
    let receipt = Receipt {
        schema_version: 1,
        version_id: Bytes(p.version),
        fabric_loader: p.fabric.map(Bytes),
        neoforge_version: p.neo.map(Bytes),
        forge_version: p.forge.map(Bytes),
        source: p.source,
        os: &target.os,
        arch: &target.arch,
    };
    let mut bytes = serde_json::to_vec(&receipt).map_err(|_| Error::named("WriteFailed"))?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub(crate) fn write(root: &ArtifactRoot, p: &Identity<'_>, target: &Target) -> Result<(), Error> {
    let bytes = encode(p, target)?;
    root.atomic_replace(".jmcl/install.json", &bytes)?;
    Ok(())
}
#[cfg(test)]
#[path = "receipt_tests.rs"]
mod tests;

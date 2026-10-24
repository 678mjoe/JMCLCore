//! Exact public server manifest schema. No EULA acceptance or running server actions.
use serde::{Deserialize, Serialize};
fn one() -> u32 {
    1
}
fn official() -> String {
    "official".into()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    #[serde(default = "one", deserialize_with = "crate::instances::schema_number")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version_id: String,
    #[serde(default)]
    pub fabric_loader: Option<String>,
    #[serde(default)]
    pub neoforge_version: Option<String>,
    #[serde(default)]
    pub forge_version: Option<String>,
    #[serde(default = "official")]
    pub source: String,
    #[serde(default)]
    pub accept_eula: bool,
    #[serde(default)]
    pub java_path: Option<String>,
}
impl Manifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        crate::instances::validate_id(&self.id).map_err(|_| "InvalidServerId")?;
        if self.schema_version != 1 {
            return Err("UnsupportedServerSchema");
        }
        if self.name.is_empty() || self.name.len() > 256 {
            return Err("InvalidServerName");
        }
        if self.version_id.is_empty() || self.version_id.len() > 256 {
            return Err("InvalidVersionId");
        }
        for (v, e) in [
            (&self.fabric_loader, "InvalidFabricLoaderVersion"),
            (&self.neoforge_version, "InvalidNeoForgeVersion"),
            (&self.forge_version, "InvalidForgeVersion"),
        ] {
            if let Some(v) = v
                && (v.is_empty() || v.len() > 256)
            {
                return Err(e);
            }
        }
        if [
            &self.fabric_loader,
            &self.neoforge_version,
            &self.forge_version,
        ]
        .iter()
        .filter(|x| x.is_some())
        .count()
            > 1
        {
            return Err("LoaderConflict");
        }
        if let Some(p) = &self.java_path
            && (p.is_empty() || p.len() > 4096 || p.contains('\0'))
        {
            return Err("InvalidJavaPath");
        }
        if !matches!(self.source.as_str(), "official" | "bmclapi") {
            return Err("InvalidEnumTag");
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], id: &str) -> Result<Self, &'static str> {
        crate::instances::validate_id(id).map_err(|_| "InvalidServerId")?;
        if bytes.len() > 64 * 1024 {
            return Err("ServerManifestTooLarge");
        }
        let m: Self = crate::instances::parse_document(bytes)?;
        m.validate()?;
        if m.id != id {
            return Err("ServerIdMismatch");
        }
        Ok(m)
    }
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self).map_err(|_| "InvalidServerManifest")?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_disk_format_defaults_roundtrip() {
        let m = Manifest::parse(br#"{"id":"a","name":"A","version_id":"1.21"}"#, "a").unwrap();
        assert!(!m.accept_eula);
        assert_eq!(m.java_path, None);
        assert_eq!(Manifest::parse(&m.encode().unwrap(), "a").unwrap(), m);
    }
    #[test]
    fn loader_and_java_validation() {
        assert!(
            Manifest::parse(
                br#"{"id":"a","name":"A","version_id":"1.21","java_path":""}"#,
                "a"
            )
            .is_err()
        );
    }
}

//! Shared planning selection and completed-installation receipt.
use super::Error;
use crate::{
    metadata::{self, Source},
    plan::{Plan, Target},
    store::ArtifactRoot,
    version,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Selection {
    pub id: String,
    #[serde(default)]
    pub fabric_loader: Option<String>,
    #[serde(default)]
    pub neoforge_version: Option<String>,
    #[serde(default)]
    pub forge_version: Option<String>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}
impl Selection {
    pub fn target(&self) -> Result<Target, metadata::Error> {
        let mut target = Target::host()?;
        if let Some(v) = &self.os {
            target.os = v.clone();
        }
        if let Some(v) = &self.arch {
            target.arch = v.clone();
        }
        Ok(target)
    }
    pub fn valid_target(&self) -> bool {
        self.os
            .as_deref()
            .is_none_or(|s| matches!(s, "linux" | "windows" | "osx"))
            && self
                .arch
                .as_deref()
                .is_none_or(|s| matches!(s, "x86" | "x86_64" | "arm" | "arm64"))
    }
}
pub trait Planner {
    fn plan(&self, selection: &Selection, target: Target) -> Result<Plan, metadata::Error>;
}
impl<T: version::Transport> Planner for version::Service<T> {
    fn plan(&self, p: &Selection, target: Target) -> Result<Plan, metadata::Error> {
        if [&p.fabric_loader, &p.neoforge_version, &p.forge_version]
            .iter()
            .filter(|v| v.is_some())
            .count()
            > 1
        {
            return Err(metadata::Error::Plan("LoaderConflict"));
        }
        if let Some(v) = &p.forge_version {
            self.plan_forge(&p.id, v, p.source, target)
        } else if let Some(v) = &p.neoforge_version {
            self.plan_neoforge(&p.id, v, p.source, target)
        } else {
            self.plan_loader(&p.id, p.fabric_loader.as_deref(), p.source, target)
        }
    }
}
pub fn record_completed(root: &ArtifactRoot, p: &Selection, target: &Target) -> Result<(), Error> {
    if p.id.is_empty() || p.id.len() > 256 {
        return Err(Error::named("InvalidVersionId"));
    }
    let loaders = [&p.fabric_loader, &p.neoforge_version, &p.forge_version];
    for v in loaders.iter().filter_map(|v| v.as_ref()) {
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
        version_id: &'a str,
        fabric_loader: Option<&'a str>,
        neoforge_version: Option<&'a str>,
        forge_version: Option<&'a str>,
        source: Source,
        os: &'a str,
        arch: &'a str,
    }
    let receipt = Receipt {
        schema_version: 1,
        version_id: &p.id,
        fabric_loader: p.fabric_loader.as_deref(),
        neoforge_version: p.neoforge_version.as_deref(),
        forge_version: p.forge_version.as_deref(),
        source: p.source,
        os: &target.os,
        arch: &target.arch,
    };
    let mut bytes = serde_json::to_vec(&receipt).map_err(|_| Error::named("WriteFailed"))?;
    bytes.push(b'\n');
    root.atomic_replace(".jmcl/install.json", &bytes)?;
    Ok(())
}

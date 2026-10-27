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
    super::receipt::write(
        root,
        &super::receipt::Identity {
            version: p.id.as_bytes(),
            fabric: p.fabric_loader.as_deref().map(str::as_bytes),
            neo: p.neoforge_version.as_deref().map(str::as_bytes),
            forge: p.forge_version.as_deref().map(str::as_bytes),
            source: p.source,
        },
        target,
    )
}

/// Legacy Forge has no processor chain. Do not infer a chain merely from the
/// request's forge_version: the resolved plan is the source of truth.
pub fn processor_loader<'a>(
    plan: &Plan,
    selection: &'a Selection,
) -> Option<crate::loaders::Loader<'a>> {
    match plan.loader.as_str() {
        "forge" => {
            selection
                .forge_version
                .as_deref()
                .map(|version| crate::loaders::Loader::Forge {
                    minecraft: &selection.id,
                    version,
                })
        }
        "neoforge" => selection
            .neoforge_version
            .as_deref()
            .map(|version| crate::loaders::Loader::NeoForge { version }),
        _ => None,
    }
}
/// Preserve policy resolution before the chain (including its cached shortcut).
/// Missing runtime provisioning is explicit: archive installation is not part
/// of this slice and must never become a synthetic success.
pub fn processor_java(
    plan: &Plan,
    paths: &[String],
    policy: &str,
    store: &std::path::Path,
    environment: &crate::java::detect::Environment,
) -> Result<String, &'static str> {
    use crate::java::managed::{self, Policy};
    let policy = match policy {
        "auto" => Policy::Auto,
        "local" => Policy::Local,
        "managed" => Policy::Managed,
        _ => return Err("InvalidInput"),
    };
    let request = managed::Request {
        required_major: plan.java_major_version,
        component: None,
        arch: plan.target.arch.clone(),
        policy,
        java_paths: paths.to_vec(),
        java_override: None,
        store_directory: store.to_string_lossy().into_owned(),
        allow_download: true,
    };
    managed::resolve(Some(environment), &request, &mut |_, _| {
        Err("NotImplemented")
    })
    .map(|r| r.runtime.executable)
}

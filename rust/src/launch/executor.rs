//! Native launch orchestration. Preparation owns its cleanup guard through every
//! Java-selection, planning and process error. No downloader or Java policy is faked.
use super::{Auth, Options, Plan, process};
use crate::{
    install,
    java::{detect, managed, select},
    metadata::Source,
    plan,
    store::ArtifactRoot,
    version::{Service, Transport},
};
use serde::Serialize;
pub type Error = &'static str;
pub struct Input<'a> {
    pub id: &'a str,
    pub fabric_loader: Option<&'a str>,
    pub neoforge_version: Option<&'a str>,
    pub forge_version: Option<&'a str>,
    pub directory: &'a str,
    pub auth: Auth<'a>,
    pub options: Options<'a>,
    pub source: Source,
    pub arch: Option<&'a str>,
    pub java_paths: &'a [&'a str],
    pub java_policy: managed::Policy,
    pub java_override: Option<&'a str>,
    pub store_directory: &'a str,
}
pub struct Prepared<'a> {
    root: ArtifactRoot,
    native_relative: String,
    pub plan: Plan,
    secrets: [&'a str; 2],
    cleanup_needed: bool,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub process: process::ResultData,
    pub native_cleanup: &'static str,
}
impl Drop for Prepared<'_> {
    fn drop(&mut self) {
        if self.cleanup_needed {
            let _ = install::cleanup_native(&self.root, &self.native_relative);
        }
    }
}
impl Prepared<'_> {
    pub fn run(
        mut self,
        env: &process::Environment,
        mut reporter: impl FnMut(process::Event<'_>, &Plan),
    ) -> Result<ResultData, Error> {
        let argv = self.plan.argv().collect::<Vec<_>>();
        let process = process::run(
            &argv,
            &self.plan.working_directory,
            &self.secrets,
            env,
            |event| reporter(event, &self.plan),
        )
        .map_err(|_| "ProcessFailed")?;
        let native_cleanup = if install::cleanup_native(&self.root, &self.native_relative).is_ok() {
            "removed"
        } else {
            "failed"
        };
        self.cleanup_needed = false;
        Ok(ResultData {
            process,
            native_cleanup,
        })
    }
}
/// The injected resolver is the actual source-aware Java policy implementation;
/// tests can supply a native fake Java resolution without installing any runtime.
pub fn prepare<'a, T: Transport>(
    service: &Service<T>,
    input: Input<'a>,
    env: Option<&detect::Environment>,
    mut resolve_java: impl FnMut(&managed::Request) -> Result<managed::Resolution, Error>,
) -> Result<Prepared<'a>, Error> {
    if input.id.is_empty()
        || input.directory.is_empty()
        || input.auth.player_name.is_empty()
        || input.auth.uuid.is_empty()
        || input.auth.access_token.is_empty()
        || input.auth.access_token.len() > 16384
    {
        return Err("InvalidInput");
    }
    let mut target = plan::Target::host().map_err(|_| "UnsupportedPlatform")?;
    if let Some(arch) = input.arch {
        target.arch = arch.into()
    }
    if [
        input.fabric_loader,
        input.neoforge_version,
        input.forge_version,
    ]
    .iter()
    .flatten()
    .count()
        > 1
    {
        return Err("MetadataFailed");
    }
    let metadata = if let Some(f) = input.forge_version {
        service
            .resolve_forge(input.id, f, input.source)
            .map(|r| r.metadata)
    } else if let Some(n) = input.neoforge_version {
        service
            .resolve_neoforge(input.id, n, input.source)
            .map(|r| r.0)
    } else {
        service.resolve(input.id, input.fabric_loader, input.source)
    }
    .map_err(|_| "MetadataFailed")?;
    let assets = if !metadata["assetIndex"].is_null() {
        Some(
            service
                .assets(&metadata["assetIndex"], input.source)
                .map_err(|_| "AssetIndexFailed")?,
        )
    } else {
        None
    };
    let install = plan::build(&metadata, assets.as_ref(), input.source, target.clone())
        .map_err(|_| "InstallPlanFailed")?;
    let root = ArtifactRoot::open(input.directory).map_err(|_| "DirectoryFailed")?;
    let absolute = root.absolute_path().map_err(|_| "DirectoryFailed")?;
    let absolute = absolute.to_str().ok_or("DirectoryFailed")?;
    let preparation = install::prepare(&root, &install).map_err(|_| "PrepareFailed")?;
    let native_relative = preparation.native_directory.ok_or("PrepareFailed")?;
    let secrets = [input.auth.access_token, input.auth.session.unwrap_or("")];
    // Keep cleanup live even while metadata or resolver validation fails.
    let result = (|| {
        let native = root
            .absolute_directory(&native_relative)
            .map_err(|_| "PrepareFailed")?;
        let native = native.to_str().ok_or("PrepareFailed")?;
        let required = select::required_major(&metadata).map_err(|_| "InvalidMetadata")?;
        env.ok_or("EnvironmentUnavailable")?;
        let java = resolve_java(&managed::Request {
            required_major: required,
            component: metadata["javaVersion"]["component"]
                .as_str()
                .map(Into::into),
            arch: target.arch,
            policy: input.java_policy,
            java_paths: input.java_paths.iter().map(|s| (*s).into()).collect(),
            java_override: input.java_override.map(Into::into),
            store_directory: input.store_directory.into(),
            allow_download: true,
        })?;
        super::build(super::Input {
            metadata: &metadata,
            install: &install,
            runtime: &java.runtime,
            root_directory: absolute,
            game_directory: absolute,
            natives_directory: native,
            auth: input.auth,
            options: input.options,
            force_java: matches!(java.source, managed::Source::Forced),
        })
        .map_err(|_| "LaunchPlanFailed")
    })();
    match result {
        Ok(plan) => Ok(Prepared {
            root,
            native_relative,
            plan,
            secrets,
            cleanup_needed: true,
        }),
        Err(e) => {
            let _ = install::cleanup_native(&root, &native_relative);
            Err(e)
        }
    }
}

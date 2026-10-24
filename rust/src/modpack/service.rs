//! Transactional modpack installation. Failed attempts remove only the newly
//! created instance; verified CAS objects remain reusable on retry.
use super::{
    Error,
    planning::{self, CurseforgeProvider, Downloads, Pack},
    schema::Loader,
};
use crate::{
    install,
    instances::{Manifest, Root},
    store::{ArtifactRoot, Store},
};
use serde::Serialize;
use std::io::Read;
#[derive(Debug, Serialize)]
pub struct LoaderSummary {
    pub name: String,
    pub version: String,
}
#[derive(Debug, Serialize)]
pub struct Summary {
    pub stage: &'static str,
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub loader: Option<LoaderSummary>,
    pub files_installed: usize,
    pub files_skipped: usize,
    pub overrides_written: usize,
}
pub struct Request<'a> {
    pub directory: &'a str,
    pub store_directory: &'a str,
    pub id: &'a str,
    pub file: &'a str,
    pub api_key: Option<&'a str>,
}
fn io(e: std::io::Error) -> Error {
    Error(crate::instances::cli_error_name(&e))
}
impl From<install::Error> for Error {
    fn from(e: install::Error) -> Self {
        Self(e.0)
    }
}
/// All filesystem roots are opened before reading the pack, matching RPC/CLI.
/// The provider receives no key here: authentication is owned by the caller.
pub fn install<T: install::Transport>(
    request: Request<'_>,
    options: install::Options,
    provider: &mut impl CurseforgeProvider,
    transport: &T,
    reporter: Option<&(dyn Fn(install::Event) -> Result<(), install::Error> + Sync)>,
) -> Result<Summary, Error> {
    crate::instances::validate_id(request.id).map_err(Error::from)?;
    let root = Root::open(request.directory)
        .map_err(|e| Error(format!("InstancesRootError:{}", io(e).name())))?;
    let store = Store::open(request.store_directory)
        .map_err(|e| Error(format!("StoreRootError:{}", io(e).name())))?;
    if let Some(key) = request.api_key {
        super::provider::validate_key(key)?;
    }
    let file = std::fs::File::open(request.file).map_err(io)?;
    let mut bytes = Vec::new();
    file.take(planning::MAX_PACK_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > planning::MAX_PACK_BYTES {
        return Err("ModpackTooLarge".into());
    }
    if root.contains_directory(request.id).map_err(io)? {
        return Err("InstanceAlreadyExists".into());
    }
    let pack = Pack::inspect(&bytes, request.api_key.is_some())?;
    let (name, version_id, loader) = pack.identity()?;
    let mut manifest = Manifest {
        schema_version: 1,
        id: request.id.into(),
        name: name.clone(),
        version_id: version_id.clone(),
        fabric_loader: None,
        neoforge_version: None,
        forge_version: None,
        source: "official".into(),
    };
    if let Some(s) = &loader {
        match s.loader {
            Loader::Fabric => manifest.fabric_loader = Some(s.version.clone()),
            Loader::Forge => manifest.forge_version = Some(s.version.clone()),
            Loader::Neoforge => manifest.neoforge_version = Some(s.version.clone()),
        }
    }
    root.create(&manifest).map_err(io)?;
    let result = (|| {
        let game = root.game_root(request.id).map_err(io)?;
        let downloads = match &pack {
            Pack::Modrinth(p) => Downloads::mrpack(p),
            Pack::Curseforge(p) => Downloads::curseforge(p, provider)?,
        };
        install::execute(
            &game,
            &store,
            &downloads.plan(&version_id)?,
            options,
            transport,
            reporter,
        )?;
        save_registry(&game, &downloads.entries)?;
        let mut overrides_written = 0;
        for dir in pack.override_directories() {
            overrides_written += planning::overrides_bytes(&bytes, dir, |path, data| {
                game.atomic_replace_bytes(path, data).map_err(io)
            })?;
        }
        Ok(Summary {
            stage: "modpack.install",
            id: request.id.into(),
            name,
            version_id,
            loader: loader.map(|s| LoaderSummary {
                name: s.loader.name().into(),
                version: s.version,
            }),
            files_installed: downloads.files.len(),
            files_skipped: downloads.skipped,
            overrides_written,
        })
    })();
    if result.is_err() {
        let _ = root.delete(request.id);
    }
    result
}
fn save_registry(
    game: &ArtifactRoot,
    entries: &[crate::content::registry::Entry],
) -> Result<(), Error> {
    let registry = crate::content::registry::Registry {
        schema_version: 1,
        entries: entries.to_vec(),
    };
    registry.validate().map_err(Error::from)?;
    let mut bytes = serde_json::to_vec(&registry).map_err(|_| Error::from("Unexpected"))?;
    bytes.push(b'\n');
    game.atomic_replace(".jmcl/content.json", &bytes)
        .map_err(io)
}

#[cfg(all(test, unix))]
mod native_error_tests {
    #[test]
    fn raw_name_error_keeps_modpack_boundary() {
        let error = super::io(std::io::Error::from_raw_os_error(
            rustix::io::Errno::ILSEQ.raw_os_error(),
        ));
        assert_eq!(error.name(), "BadPathName");
        let missing = super::io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(missing.name(), "FileNotFound");
    }
}

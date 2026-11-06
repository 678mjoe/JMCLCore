//! Transactional modpack installation. Failed attempts remove only the newly
//! created instance; verified CAS objects remain reusable on retry.
use super::{
    Error,
    planning::{self, CurseforgeProvider, Downloads, Pack},
    schema::Loader,
};
use crate::{
    install,
    instances::{DisplayName, ReadManifest, Root},
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
    pub name: crate::content::modrinth::MetadataBytes,
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
fn pack_manifest(
    id: &str,
    name: &crate::content::modrinth::MetadataBytes,
    version_id: &str,
    loader: Option<&super::Selection>,
) -> ReadManifest {
    let mut manifest = ReadManifest {
        schema_version: 1,
        id: id.into(),
        name: DisplayName(name.clone().into_bytes()),
        version_id: version_id.into(),
        fabric_loader: None,
        neoforge_version: None,
        forge_version: None,
        source: "official".into(),
    };
    if let Some(s) = loader {
        match s.loader {
            Loader::Fabric => manifest.fabric_loader = Some(s.version.clone()),
            Loader::Forge => manifest.forge_version = Some(s.version.clone()),
            Loader::Neoforge => manifest.neoforge_version = Some(s.version.clone()),
        }
    }
    manifest
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
    let manifest = pack_manifest(request.id, &name, &version_id, loader.as_ref());
    root.create_read(&manifest).map_err(io)?;
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

#[cfg(test)]
mod display_byte_tests {
    use super::*;
    #[test]
    fn display_identity_manifest_and_summary_preserve_bytes() {
        for (name, version, expected) in [
            (r#""Plain""#, r#""1.0""#, b"Plain 1.0".as_slice()),
            ("[65,255]", "[66,254]", b"A\xff B\xfe".as_slice()),
            ("[65,0]", "null", b"A\0".as_slice()),
        ] {
            let mr = format!(
                r#"{{"formatVersion":1,"game":"minecraft","name":{name},"versionId":{version},"summary":[255],"files":[],"dependencies":{{"minecraft":"1.20.1"}}}}"#
            );
            let cf = format!(
                r#"{{"manifestVersion":1,"manifestType":"minecraftModpack","name":{name},"version":{version},"author":[255],"files":[],"minecraft":{{"version":"1.20.1"}}}}"#
            );
            for pack in [
                Pack::Modrinth(super::super::schema::Mrpack::parse(mr.as_bytes()).unwrap()),
                Pack::Curseforge(super::super::schema::Curseforge::parse(cf.as_bytes()).unwrap()),
            ] {
                let (name, version_id, loader) = pack.identity().unwrap();
                assert_eq!(name.as_bytes(), expected);
                let manifest = pack_manifest("demo", &name, &version_id, loader.as_ref());
                let encoded = manifest.encode().unwrap();
                if std::str::from_utf8(expected).is_ok() {
                    assert_eq!(
                        crate::instances::Manifest::parse(&encoded, "demo")
                            .unwrap()
                            .encode()
                            .unwrap(),
                        encoded
                    );
                }
                assert_eq!(
                    ReadManifest::parse(&encoded, "demo").unwrap().name.0,
                    expected
                );
                let summary = Summary {
                    stage: "modpack.install",
                    id: "demo".into(),
                    name,
                    version_id,
                    loader: None,
                    files_installed: 0,
                    files_skipped: 0,
                    overrides_written: 0,
                };
                let value = serde_json::to_value(summary).unwrap();
                let expected_json = match std::str::from_utf8(expected) {
                    Ok(s) => serde_json::json!(s),
                    Err(_) => serde_json::json!(expected),
                };
                assert_eq!(value["name"], expected_json);
            }
        }
    }
    #[test]
    fn display_limits_preserve_schema_then_instance_validation() {
        for (n, v, valid) in [(254, 1, true), (255, 1, false), (256, 0, true)] {
            let version = if v == 0 {
                "null".into()
            } else {
                format!("\"{}\"", "v".repeat(v))
            };
            let raw = format!(
                r#"{{"formatVersion":1,"game":"minecraft","name":"{}","versionId":{version},"files":[],"dependencies":{{"minecraft":"1.20.1"}}}}"#,
                "n".repeat(n)
            );
            let cf = format!(
                r#"{{"manifestVersion":1,"manifestType":"minecraftModpack","name":"{}","version":{version},"files":[],"minecraft":{{"version":"1.20.1"}}}}"#,
                "n".repeat(n)
            );
            for pack in [
                Pack::Modrinth(super::super::schema::Mrpack::parse(raw.as_bytes()).unwrap()),
                Pack::Curseforge(super::super::schema::Curseforge::parse(cf.as_bytes()).unwrap()),
            ] {
                let (name, version_id, loader) = pack.identity().unwrap();
                let manifest = pack_manifest("demo", &name, &version_id, loader.as_ref());
                if valid {
                    assert!(manifest.encode().is_ok());
                } else {
                    assert_eq!(manifest.encode(), Err("InvalidInstanceName"));
                }
            }
        }
        for (name, version, error) in [
            ("[]", "null", "InvalidModpackName"),
            ("[65]", "[]", "InvalidModpackVersion"),
        ] {
            let raw = format!(
                r#"{{"formatVersion":1,"game":"minecraft","name":{name},"versionId":{version},"files":[],"dependencies":{{"minecraft":"1.20.1"}}}}"#
            );
            let cf = format!(
                r#"{{"manifestVersion":1,"manifestType":"minecraftModpack","name":{name},"version":{version},"files":[],"minecraft":{{"version":"1.20.1"}}}}"#
            );
            assert_eq!(
                super::super::schema::Curseforge::parse(cf.as_bytes())
                    .unwrap_err()
                    .name(),
                error
            );
            assert_eq!(
                super::super::schema::Mrpack::parse(raw.as_bytes())
                    .unwrap_err()
                    .name(),
                error
            );
        }
    }
    #[test]
    fn opaque_manifest_preserves_each_loader_mapping() {
        for kind in [Loader::Fabric, Loader::Forge, Loader::Neoforge] {
            let selected = super::super::Selection {
                loader: kind,
                version: "v1".into(),
            };
            let m = pack_manifest("demo", &vec![255].into(), "1.20.1", Some(&selected));
            let decoded = ReadManifest::parse(&m.encode().unwrap(), "demo").unwrap();
            assert_eq!(
                decoded.fabric_loader.as_deref(),
                (kind == Loader::Fabric).then_some("v1")
            );
            assert_eq!(
                decoded.forge_version.as_deref(),
                (kind == Loader::Forge).then_some("v1")
            );
            assert_eq!(
                decoded.neoforge_version.as_deref(),
                (kind == Loader::Neoforge).then_some("v1")
            );
            assert_eq!(decoded.name.0, [255]);
        }
    }
}

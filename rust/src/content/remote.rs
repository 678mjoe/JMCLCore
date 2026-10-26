//! Injected metadata resolution and immutable content installation.
//! Provider credentials never enter this provider-neutral plan.
use super::{
    Root, modrinth as mr,
    registry::{
        EntryRecord, Kind, ListingRegistry, Provider, Registry, RegistryFile, RegistryRecord,
        Source,
    },
};
use crate::{install, plan, store::Store};
use serde::Serialize;
use std::collections::HashSet;
pub trait Metadata {
    fn get(&self, url: &str, limit: usize) -> mr::Result<Vec<u8>>;
}
#[derive(Clone, Debug)]
pub struct ResolvedRecord<F> {
    pub project_id: String,
    pub slug: Option<String>,
    pub version_id: String,
    pub file_name: F,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub as_dependency: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct InstalledRecord<P> {
    pub file: P,
    pub sha1: String,
    pub size: u64,
    pub slug: Option<String>,
    pub as_dependency: bool,
}
#[derive(Debug, Serialize)]
pub struct SummaryRecord<P> {
    pub installed: Vec<InstalledRecord<P>>,
    pub skipped: usize,
}
pub struct ResolutionRecord<F> {
    pub files: Vec<ResolvedRecord<F>>,
    pub skipped: usize,
}
pub type Installed = InstalledRecord<String>;
pub type Summary = SummaryRecord<String>;
pub(crate) type ByteSummary = SummaryRecord<super::ListedPath>;
trait OutputPath: RegistryFile {
    fn from_relative(bytes: Vec<u8>) -> Result<Self, install::Error>;
}
impl OutputPath for String {
    fn from_relative(bytes: Vec<u8>) -> Result<Self, install::Error> {
        String::from_utf8(bytes).map_err(|_| install::Error::named("UnexpectedToken"))
    }
}
impl OutputPath for super::ListedPath {
    fn from_relative(bytes: Vec<u8>) -> Result<Self, install::Error> {
        Ok(Self(bytes))
    }
}
fn content_path<P: OutputPath>(kind: Kind, name: &[u8]) -> Result<P, install::Error> {
    let mut bytes = kind.directory().as_bytes().to_vec();
    bytes.push(b'/');
    bytes.extend_from_slice(name);
    P::from_relative(bytes)
}
fn string_event(event: install::ByteEvent) -> Result<install::Event, install::Error> {
    let path = |bytes: Vec<u8>| {
        String::from_utf8(bytes).map_err(|_| install::Error::named("UnexpectedToken"))
    };
    Ok(match event {
        install::ByteEvent::Progress(p) => install::Event::Progress(p),
        install::ByteEvent::File(f) => install::Event::File(install::FileEvent {
            path: path(f.path.0)?,
            status: f.status,
            placement: f.placement,
            size: f.size,
            attempts: f.attempts,
        }),
        install::ByteEvent::Retry(r) => install::Event::Retry(install::RetryEvent {
            path: path(r.path.0)?,
            attempt: r.attempt,
            max_attempts: r.max_attempts,
            error_name: r.error_name,
        }),
    })
}
pub type Resolved = ResolvedRecord<String>;
pub type Resolution = ResolutionRecord<String>;
pub(crate) type ByteResolution = ResolutionRecord<mr::MetadataBytes>;
struct Resolver<'a, T, F, R: super::registry::RegistryFile> {
    transport: &'a T,
    kind: Kind,
    game: &'a str,
    loader: Option<&'a str>,
    dependencies: bool,
    registry: &'a super::registry::RegistryRecord<R>,
    seen: HashSet<String>,
    result: ResolutionRecord<F>,
}
impl<T: Metadata, F: mr::FilenameValue + Clone, R: super::registry::RegistryFile>
    Resolver<'_, T, F, R>
{
    fn project(&self, id: &str) -> mr::Result<mr::Project> {
        mr::parse_project(&self.transport.get(&mr::project_url(id)?, 1024 * 1024)?)
    }
    fn version(&self, id: &str) -> mr::Result<mr::VersionRecord<F>> {
        mr::parse_version_as(&self.transport.get(&mr::version_url(id)?, 4 * 1024 * 1024)?)
    }
    fn resolve(
        &mut self,
        project: &str,
        version: Option<&str>,
        dependency: bool,
        depth: u8,
    ) -> mr::Result<()> {
        if depth > 32 {
            return Err("ContentDependencyTooDeep".into());
        }
        if self.result.files.len() >= 256 {
            return Err("TooManyContentDependencies".into());
        }
        let p = self.project(project)?;
        let matching = matches!(
            (self.kind, p.project_type.as_str()),
            (Kind::Mod, "mod")
                | (Kind::Resourcepack, "resourcepack")
                | (Kind::Shaderpack, "shader")
                | (Kind::Datapack, "datapack")
        ) || (self.kind == Kind::Datapack
            && p.project_type == "mod"
            && p.loaders.iter().any(|l| l == "datapack"));
        if !matching {
            return Err("ContentKindMismatch".into());
        }
        if !self.seen.insert(p.id.clone()) {
            return Ok(());
        }
        if self.registry.entries.iter().any(|e| {
            e.source.provider == Provider::Modrinth && e.source.project_id.as_deref() == Some(&p.id)
        }) {
            self.result.skipped += 1;
            return Ok(());
        }
        let v = if let Some(id) = version {
            let v = self.version(id)?;
            if v.project_id != p.id {
                return Err("ModrinthVersionProjectMismatch".into());
            }
            if !mr::supports(&v, self.game, self.loader) {
                return Err("IncompatibleContentVersion".into());
            }
            v
        } else {
            let list = mr::parse_versions_as(&self.transport.get(
                &mr::versions_url(&p.id, self.game, self.loader)?,
                16 * 1024 * 1024,
            )?)?;
            mr::select(&list, self.game, self.loader)
                .ok_or_else(|| mr::Error::from("NoCompatibleContentVersion"))?
                .clone()
        };
        let f = mr::select_file_as(
            &v,
            if self.kind == Kind::Datapack {
                Some(".zip")
            } else {
                None
            },
        )
        .ok_or_else(|| mr::Error::from("ContentFileSelectionFailed"))?;
        self.result.files.push(ResolvedRecord {
            project_id: p.id,
            slug: Some(p.slug),
            version_id: v.id.clone(),
            file_name: f.filename.clone(),
            url: f.url.clone(),
            sha1: f.hashes.sha1.clone(),
            size: f.size,
            as_dependency: dependency,
        });
        if self.dependencies {
            for d in v.dependencies {
                if d.dependency_type != mr::DependencyType::Required {
                    continue;
                }
                if let Some(p) = d.project_id {
                    self.resolve(&p, None, true, depth + 1)?
                } else if let Some(id) = d.version_id {
                    let v = self.version(&id)?;
                    self.resolve(&v.project_id, Some(&id), true, depth + 1)?
                }
            }
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
fn resolve_modrinth_as<
    T: Metadata,
    F: mr::FilenameValue + Clone,
    R: super::registry::RegistryFile,
>(
    transport: &T,
    kind: Kind,
    game: &str,
    loader: Option<&str>,
    project: &str,
    version: Option<&str>,
    dependencies: bool,
    registry: &super::registry::RegistryRecord<R>,
) -> mr::Result<ResolutionRecord<F>> {
    mr::project_ref(project)?;
    mr::game_version(game)?;
    if let Some(v) = version {
        mr::version_id(v)?
    }
    let effective = match kind {
        Kind::Mod => {
            let l = loader.ok_or_else(|| mr::Error::from("ModLoaderRequired"))?;
            if !matches!(l, "fabric" | "forge" | "neoforge") {
                return Err("InvalidContentLoader".into());
            }
            Some(l)
        }
        Kind::Resourcepack => {
            if loader.is_some_and(|l| l != "minecraft") {
                return Err("InvalidContentLoader".into());
            }
            Some("minecraft")
        }
        Kind::Shaderpack | Kind::Datapack => {
            if loader.is_some() {
                return Err("InvalidContentLoader".into());
            }
            if kind == Kind::Datapack {
                Some("datapack")
            } else {
                None
            }
        }
    };
    let mut r = Resolver {
        transport,
        kind,
        game,
        loader: effective,
        dependencies,
        registry,
        seen: HashSet::new(),
        result: ResolutionRecord {
            files: vec![],
            skipped: 0,
        },
    };
    r.resolve(project, version, false, 0)?;
    Ok(r.result)
}
#[allow(clippy::too_many_arguments)]
pub fn resolve_modrinth<T: Metadata>(
    transport: &T,
    kind: Kind,
    game: &str,
    loader: Option<&str>,
    project: &str,
    version: Option<&str>,
    dependencies: bool,
    registry: &Registry,
) -> mr::Result<Resolution> {
    resolve_modrinth_as(
        transport,
        kind,
        game,
        loader,
        project,
        version,
        dependencies,
        registry,
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_modrinth_bytes<T: Metadata>(
    transport: &T,
    kind: Kind,
    game: &str,
    loader: Option<&str>,
    project: &str,
    version: Option<&str>,
    dependencies: bool,
    registry: &super::registry::ListingRegistry,
) -> mr::Result<ByteResolution> {
    resolve_modrinth_as(
        transport,
        kind,
        game,
        loader,
        project,
        version,
        dependencies,
        registry,
    )
}
impl Root {
    #[allow(clippy::too_many_arguments)]
    pub fn install_resolved<T: install::Transport>(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        provider: Provider,
        resolved: Resolution,
        options: install::Options,
        transport: &T,
        reporter: Option<&(dyn Fn(install::Event) -> Result<(), install::Error> + Sync)>,
    ) -> Result<Summary, install::Error> {
        let registry = self
            .load()
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        self.install_resolved_with_registry(
            store, kind, game, provider, resolved, registry, options, transport, reporter,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn install_resolved_record<T: install::Transport, F: mr::FilenameValue, P: OutputPath>(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        provider: Provider,
        resolved: ResolutionRecord<F>,
        mut registry: RegistryRecord<P>,
        options: install::Options,
        transport: &T,
        reporter: Option<&(dyn Fn(install::ByteEvent) -> Result<(), install::Error> + Sync)>,
    ) -> Result<SummaryRecord<P>, install::Error> {
        let mut files = vec![];
        let mut total = 0u64;
        for c in &resolved.files {
            if kind == Kind::Datapack && !c.file_name.filename_bytes().ends_with(b".zip") {
                return Err(install::Error::named("ContentFileSelectionFailed"));
            }
            let path: P = content_path(kind, c.file_name.filename_bytes())?;
            if registry
                .entries
                .iter()
                .any(|e| e.file == path && !e.sha1.eq_ignore_ascii_case(&c.sha1))
            {
                return Err(install::Error::named("ContentFileConflict"));
            }
            total = total
                .checked_add(c.size)
                .ok_or_else(|| install::Error::named("DownloadSizeOverflow"))?;
            files.push(install::ByteFile {
                path: install::BytePath(path.file_bytes().to_vec()),
                url: c.url.clone(),
                fallback_url: None,
                sha1: c.sha1.clone(),
                size: c.size,
            });
        }
        let _ = game;
        plan::Target::host().map_err(|e| install::Error::named(e.name()))?;
        install::execute_bytes(
            &self.0.artifact_root()?,
            store,
            &files,
            total,
            options,
            transport,
            reporter,
        )?;
        let mut installed = vec![];
        for (c, f) in resolved.files.into_iter().zip(files) {
            registry.entries.push(EntryRecord {
                kind,
                file: P::from_relative(f.path.0.clone())?,
                sha1: c.sha1.clone(),
                size: c.size,
                source: Source {
                    provider,
                    project_id: Some(c.project_id),
                    version_id: Some(c.version_id),
                    slug: c.slug.clone(),
                },
                enabled: true,
                as_dependency: c.as_dependency,
            });
            installed.push(InstalledRecord {
                file: P::from_relative(f.path.0)?,
                sha1: c.sha1,
                size: c.size,
                slug: c.slug,
                as_dependency: c.as_dependency,
            });
        }
        self.save_record(&registry)
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        Ok(SummaryRecord {
            installed,
            skipped: resolved.skipped,
        })
    }
}
#[derive(Debug, Serialize)]
pub struct VersionChangeRecord<P> {
    pub changed: bool,
    pub file: P,
    pub sha1: String,
    pub size: u64,
    pub old_version_id: String,
    pub new_version_id: String,
}
pub type VersionChange = VersionChangeRecord<String>;
pub(crate) type ByteVersionChange = VersionChangeRecord<super::ListedPath>;
impl Root {
    #[allow(clippy::too_many_arguments)]
    fn set_version_record<
        M: Metadata + super::curseforge::Metadata,
        T: install::Transport,
        F: mr::FilenameValue + Clone,
        P: OutputPath,
    >(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        loader: Option<&str>,
        project: &str,
        version: &str,
        api_key: Option<&str>,
        metadata: &M,
        transport: &T,
        options: install::Options,
        parse_registry: fn(&[u8]) -> Result<RegistryRecord<P>, &'static str>,
        reporter: Option<&(dyn Fn(install::ByteEvent) -> Result<(), install::Error> + Sync)>,
    ) -> Result<VersionChangeRecord<P>, install::Error> {
        let named = |e: mr::Error| install::Error::named(&e.0);
        mr::project_ref(project).map_err(named)?;
        mr::game_version(game).map_err(named)?;
        if let Some(key) = api_key {
            crate::modpack::provider::validate_key(key)
                .map_err(|e| install::Error::named(e.name()))?;
        }
        let effective = match kind {
            Kind::Mod => {
                let l = loader.ok_or_else(|| install::Error::named("ModLoaderRequired"))?;
                if !matches!(l, "fabric" | "forge" | "neoforge") {
                    return Err(install::Error::named("InvalidContentLoader"));
                }
                Some(l)
            }
            Kind::Resourcepack => {
                if loader.is_some_and(|l| l != "minecraft") {
                    return Err(install::Error::named("InvalidContentLoader"));
                }
                Some("minecraft")
            }
            Kind::Shaderpack | Kind::Datapack => {
                if loader.is_some() {
                    return Err(install::Error::named("InvalidContentLoader"));
                }
                if kind == Kind::Datapack {
                    Some("datapack")
                } else {
                    None
                }
            }
        };
        let mut registry = self
            .load_with(parse_registry)
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        let i = registry
            .entries
            .iter()
            .position(|e| {
                e.kind == kind
                    && e.source.provider != Provider::Manual
                    && (e.source.project_id.as_deref() == Some(project)
                        || e.source.slug.as_deref() == Some(project))
            })
            .ok_or_else(|| install::Error::named("ContentNotFound"))?;
        let old = registry.entries[i].clone();
        let old_version = old
            .source
            .version_id
            .clone()
            .ok_or_else(|| install::Error::named("InvalidContentSource"))?;
        if old_version == version {
            return Ok(VersionChangeRecord {
                changed: false,
                file: old.file,
                sha1: old.sha1,
                size: old.size,
                old_version_id: old_version,
                new_version_id: version.into(),
            });
        }
        let f = if old.source.provider == Provider::Modrinth {
            let url = mr::version_url(version).map_err(named)?;
            let v = mr::parse_version_as::<F>(
                &Metadata::get(metadata, &url, 4 * 1024 * 1024).map_err(named)?,
            )
            .map_err(named)?;
            if old.source.project_id.as_deref() != Some(&v.project_id) {
                return Err(install::Error::named("ModrinthVersionProjectMismatch"));
            }
            if !mr::supports(&v, game, effective) {
                return Err(install::Error::named("IncompatibleContentVersion"));
            }
            mr::select_file_as(
                &v,
                if kind == Kind::Datapack {
                    Some(".zip")
                } else {
                    None
                },
            )
            .ok_or_else(|| install::Error::named("ContentFileSelectionFailed"))?
            .clone()
        } else {
            use super::curseforge as cf;
            cf::id(version).map_err(named)?;
            let key = api_key.ok_or_else(|| install::Error::named("CurseforgeApiKeyRequired"))?;
            let project = old
                .source
                .project_id
                .as_deref()
                .ok_or_else(|| install::Error::named("InvalidContentSource"))?;
            let url = cf::file_url(project, version).map_err(named)?;
            let f = cf::parse_file_as::<F>(
                &cf::Metadata::get(metadata, &url, key, 2 * 1024 * 1024).map_err(named)?,
            )
            .map_err(named)?;
            if f.project.to_string() != project {
                return Err(install::Error::named("CurseforgeFileProjectMismatch"));
            }
            if !cf::supports(&f, game, effective) {
                return Err(install::Error::named("IncompatibleContentVersion"));
            }
            if !f.available {
                return Err(install::Error::named("CurseforgeDownloadUnavailable"));
            }
            if kind == Kind::Datapack && !f.name.filename_bytes().ends_with(b".zip") {
                return Err(install::Error::named("ContentFileSelectionFailed"));
            }
            let url = match f.url {
                Some(u) => u,
                None => cf::download_url(
                    &cf::Metadata::get(
                        metadata,
                        &format!("{url}/download-url"),
                        key,
                        2 * 1024 * 1024,
                    )
                    .map_err(named)?,
                )
                .map_err(named)?
                .ok_or_else(|| install::Error::named("CurseforgeDownloadUnavailable"))?,
            };
            mr::FileRecord {
                filename: f.name,
                url,
                size: f.size,
                primary: true,
                hashes: mr::Hashes {
                    sha1: f.sha1,
                    sha512: None,
                },
            }
        };
        let path: P = content_path(kind, f.filename.filename_bytes())?;
        if registry
            .entries
            .iter()
            .enumerate()
            .any(|(j, e)| j != i && e.file == path && !e.sha1.eq_ignore_ascii_case(&f.hashes.sha1))
        {
            return Err(install::Error::named("ContentFileConflict"));
        }
        plan::Target::host().map_err(|e| install::Error::named(e.name()))?;
        let files = [install::ByteFile {
            path: install::BytePath(path.file_bytes().to_vec()),
            url: f.url.clone(),
            fallback_url: None,
            sha1: f.hashes.sha1.clone(),
            size: f.size,
        }];
        install::execute_bytes(
            &self.0.artifact_root()?,
            store,
            &files,
            f.size,
            options,
            transport,
            reporter,
        )?;
        if !old.enabled {
            let (parent, name) = self.0.parent_bytes(path.file_bytes())?;
            let mut disabled = name.clone();
            disabled.extend_from_slice(b".disabled");
            if path == old.file {
                match parent.unlink_bytes(&disabled) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e.into()),
                }
            }
            parent.rename_bytes(&name, &disabled)?;
        }
        registry.entries[i] = EntryRecord {
            file: path.clone(),
            sha1: f.hashes.sha1.clone(),
            size: f.size,
            source: Source {
                version_id: Some(version.into()),
                ..old.source.clone()
            },
            ..old.clone()
        };
        self.save_record(&registry)
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        if old.file != path {
            let mut old_path = old.file.file_bytes().to_vec();
            if !old.enabled {
                old_path.extend_from_slice(b".disabled");
            }
            let (parent, name) = self.0.parent_bytes(&old_path)?;
            match parent.unlink_bytes(&name) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(VersionChangeRecord {
            changed: true,
            file: path,
            sha1: f.hashes.sha1.clone(),
            size: f.size,
            old_version_id: old_version,
            new_version_id: version.into(),
        })
    }
}

impl Root {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_resolved_with_registry<T: install::Transport>(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        provider: Provider,
        resolved: Resolution,
        registry: Registry,
        options: install::Options,
        transport: &T,
        reporter: Option<&(dyn Fn(install::Event) -> Result<(), install::Error> + Sync)>,
    ) -> Result<Summary, install::Error> {
        let bridge = |event| {
            let e = string_event(event)?;
            if let Some(r) = reporter {
                r(e)?;
            }
            Ok(())
        };
        self.install_resolved_record(
            store,
            kind,
            game,
            provider,
            resolved,
            registry,
            options,
            transport,
            reporter.map(|_| {
                &bridge as &(dyn Fn(install::ByteEvent) -> Result<(), install::Error> + Sync)
            }),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_resolved_with_registry_bytes<T: install::Transport>(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        provider: Provider,
        resolved: ByteResolution,
        registry: ListingRegistry,
        options: install::Options,
        transport: &T,
        reporter: Option<&(dyn Fn(install::ByteEvent) -> Result<(), install::Error> + Sync)>,
    ) -> Result<ByteSummary, install::Error> {
        self.install_resolved_record(
            store, kind, game, provider, resolved, registry, options, transport, reporter,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn set_version<M: Metadata + super::curseforge::Metadata, T: install::Transport>(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        loader: Option<&str>,
        project: &str,
        version: &str,
        api_key: Option<&str>,
        metadata: &M,
        transport: &T,
        options: install::Options,
        reporter: Option<&(dyn Fn(install::Event) -> Result<(), install::Error> + Sync)>,
    ) -> Result<VersionChange, install::Error> {
        let bridge = |event| {
            let e = string_event(event)?;
            if let Some(r) = reporter {
                r(e)?;
            }
            Ok(())
        };
        self.set_version_record::<M, T, String, String>(
            store,
            kind,
            game,
            loader,
            project,
            version,
            api_key,
            metadata,
            transport,
            options,
            super::registry::parse,
            reporter.map(|_| {
                &bridge as &(dyn Fn(install::ByteEvent) -> Result<(), install::Error> + Sync)
            }),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set_version_bytes<
        M: Metadata + super::curseforge::Metadata,
        T: install::Transport,
    >(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        loader: Option<&str>,
        project: &str,
        version: &str,
        api_key: Option<&str>,
        metadata: &M,
        transport: &T,
        options: install::Options,
        reporter: Option<&(dyn Fn(install::ByteEvent) -> Result<(), install::Error> + Sync)>,
    ) -> Result<ByteVersionChange, install::Error> {
        self.set_version_record::<M, T, mr::MetadataBytes, super::ListedPath>(
            store,
            kind,
            game,
            loader,
            project,
            version,
            api_key,
            metadata,
            transport,
            options,
            super::registry::parse_listing,
            reporter,
        )
    }
}

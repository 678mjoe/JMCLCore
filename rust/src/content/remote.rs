//! Injected metadata resolution and immutable content installation.
//! Provider credentials never enter this provider-neutral plan.
use super::{
    Root, modrinth as mr,
    registry::{Entry, Kind, Provider, Registry, Source},
};
use crate::{install, plan, store::Store};
use serde::Serialize;
use std::collections::HashSet;
pub trait Metadata {
    fn get(&self, url: &str, limit: usize) -> mr::Result<Vec<u8>>;
}
#[derive(Clone, Debug)]
pub struct Resolved {
    pub project_id: String,
    pub slug: Option<String>,
    pub version_id: String,
    pub file_name: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub as_dependency: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Installed {
    pub file: String,
    pub sha1: String,
    pub size: u64,
    pub slug: Option<String>,
    pub as_dependency: bool,
}
#[derive(Debug, Serialize)]
pub struct Summary {
    pub installed: Vec<Installed>,
    pub skipped: usize,
}
pub struct Resolution {
    pub files: Vec<Resolved>,
    pub skipped: usize,
}
struct Resolver<'a, T> {
    transport: &'a T,
    kind: Kind,
    game: &'a str,
    loader: Option<&'a str>,
    dependencies: bool,
    registry: &'a Registry,
    seen: HashSet<String>,
    result: Resolution,
}
impl<T: Metadata> Resolver<'_, T> {
    fn project(&self, id: &str) -> mr::Result<mr::Project> {
        mr::parse_project(&self.transport.get(&mr::project_url(id)?, 1024 * 1024)?)
    }
    fn version(&self, id: &str) -> mr::Result<mr::Version> {
        mr::parse_version(&self.transport.get(&mr::version_url(id)?, 4 * 1024 * 1024)?)
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
            let list = mr::parse_versions(&self.transport.get(
                &mr::versions_url(&p.id, self.game, self.loader)?,
                16 * 1024 * 1024,
            )?)?;
            mr::select(&list, self.game, self.loader)
                .ok_or_else(|| mr::Error::from("NoCompatibleContentVersion"))?
                .clone()
        };
        let f = mr::select_file(
            &v,
            if self.kind == Kind::Datapack {
                Some(".zip")
            } else {
                None
            },
        )
        .ok_or_else(|| mr::Error::from("ContentFileSelectionFailed"))?;
        self.result.files.push(Resolved {
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
        result: Resolution {
            files: vec![],
            skipped: 0,
        },
    };
    r.resolve(project, version, false, 0)?;
    Ok(r.result)
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
        let mut registry = self
            .load()
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        let mut files = vec![];
        let mut total = 0u64;
        for c in &resolved.files {
            if kind == Kind::Datapack && !c.file_name.ends_with(".zip") {
                return Err(install::Error::named("ContentFileSelectionFailed"));
            }
            let path = format!("{}/{}", kind.directory(), c.file_name);
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
            files.push(plan::File {
                path,
                url: c.url.clone(),
                fallback_url: None,
                sha1: c.sha1.clone(),
                size: c.size,
                usage: plan::Usage {
                    mod_file: kind == Kind::Mod,
                    resourcepack: kind == Kind::Resourcepack,
                    shaderpack: kind == Kind::Shaderpack,
                    datapack: kind == Kind::Datapack,
                    ..Default::default()
                },
            });
        }
        let plan = plan::Plan {
            version_id: game.into(),
            source: crate::metadata::Source::Official,
            target: plan::Target::host().map_err(|e| install::Error::named(e.name()))?,
            files,
            classpath: vec![],
            native_extractions: vec![],
            asset_projections: vec![],
            total_download_size: total,
            java_major_version: 0,
            loader: "vanilla".into(),
        };
        install::execute(
            &self.0.artifact_root()?,
            store,
            &plan,
            options,
            transport,
            reporter,
        )?;
        let mut installed = vec![];
        for (c, f) in resolved.files.into_iter().zip(plan.files) {
            registry.entries.push(Entry {
                kind,
                file: f.path.clone(),
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
            installed.push(Installed {
                file: f.path,
                sha1: c.sha1,
                size: c.size,
                slug: c.slug,
                as_dependency: c.as_dependency,
            });
        }
        self.save(&registry)
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        Ok(Summary {
            installed,
            skipped: resolved.skipped,
        })
    }
}
#[derive(Debug, Serialize)]
pub struct VersionChange {
    pub changed: bool,
    pub file: String,
    pub sha1: String,
    pub size: u64,
    pub old_version_id: String,
    pub new_version_id: String,
}
impl Root {
    #[allow(clippy::too_many_arguments)]
    pub fn set_modrinth_version<M: Metadata, T: install::Transport>(
        &self,
        store: &Store,
        kind: Kind,
        game: &str,
        loader: Option<&str>,
        project: &str,
        version: &str,
        metadata: &M,
        transport: &T,
        options: install::Options,
        reporter: Option<&(dyn Fn(install::Event) -> Result<(), install::Error> + Sync)>,
    ) -> Result<VersionChange, install::Error> {
        let named = |e: mr::Error| install::Error::named(&e.0);
        mr::project_ref(project).map_err(named)?;
        mr::game_version(game).map_err(named)?;
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
            .load()
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
            return Ok(VersionChange {
                changed: false,
                file: old.file,
                sha1: old.sha1,
                size: old.size,
                old_version_id: old_version,
                new_version_id: version.into(),
            });
        }
        if old.source.provider != Provider::Modrinth {
            return Err(install::Error::named("NOT_IMPLEMENTED"));
        }
        let url = mr::version_url(version).map_err(named)?;
        let v = mr::parse_version(&metadata.get(&url, 4 * 1024 * 1024).map_err(named)?)
            .map_err(named)?;
        if old.source.project_id.as_deref() != Some(&v.project_id) {
            return Err(install::Error::named("ModrinthVersionProjectMismatch"));
        }
        if !mr::supports(&v, game, effective) {
            return Err(install::Error::named("IncompatibleContentVersion"));
        }
        let f = mr::select_file(
            &v,
            if kind == Kind::Datapack {
                Some(".zip")
            } else {
                None
            },
        )
        .ok_or_else(|| install::Error::named("ContentFileSelectionFailed"))?;
        let path = format!("{}/{}", kind.directory(), f.filename);
        if registry
            .entries
            .iter()
            .enumerate()
            .any(|(j, e)| j != i && e.file == path && !e.sha1.eq_ignore_ascii_case(&f.hashes.sha1))
        {
            return Err(install::Error::named("ContentFileConflict"));
        }
        let plan = plan::Plan {
            version_id: game.into(),
            source: crate::metadata::Source::Official,
            target: plan::Target::host().map_err(|e| install::Error::named(e.name()))?,
            files: vec![plan::File {
                path: path.clone(),
                url: f.url.clone(),
                fallback_url: None,
                sha1: f.hashes.sha1.clone(),
                size: f.size,
                usage: plan::Usage {
                    mod_file: kind == Kind::Mod,
                    resourcepack: kind == Kind::Resourcepack,
                    shaderpack: kind == Kind::Shaderpack,
                    datapack: kind == Kind::Datapack,
                    ..Default::default()
                },
            }],
            classpath: vec![],
            native_extractions: vec![],
            asset_projections: vec![],
            total_download_size: f.size,
            java_major_version: 0,
            loader: "vanilla".into(),
        };
        install::execute(
            &self.0.artifact_root()?,
            store,
            &plan,
            options,
            transport,
            reporter,
        )?;
        if !old.enabled {
            let (parent, name) = self.0.parent(&path)?;
            let disabled = format!("{name}.disabled");
            if path == old.file {
                match parent.unlink(&disabled) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e.into()),
                }
            }
            parent.rename(&name, &disabled)?;
        }
        registry.entries[i] = Entry {
            file: path.clone(),
            sha1: f.hashes.sha1.clone(),
            size: f.size,
            source: Source {
                version_id: Some(version.into()),
                ..old.source.clone()
            },
            ..old.clone()
        };
        self.save(&registry)
            .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
        if old.file != path {
            let old_path = if old.enabled {
                old.file
            } else {
                format!("{}.disabled", old.file)
            };
            let (parent, name) = self.0.parent(&old_path)?;
            match parent.unlink(&name) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(VersionChange {
            changed: true,
            file: path,
            sha1: f.hashes.sha1.clone(),
            size: f.size,
            old_version_id: old_version,
            new_version_id: version.into(),
        })
    }
}

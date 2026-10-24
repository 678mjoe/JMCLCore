use super::{
    Error, Selection,
    schema::{self, Curseforge, Mrpack},
};
use crate::{
    content::registry::{Entry, Kind, Provider, Source},
    plan::{File, Plan, Target, Usage},
};
pub const MAX_PACK_BYTES: usize = 512 * 1024 * 1024;
#[derive(Debug)]
pub enum Pack {
    Modrinth(Mrpack),
    Curseforge(Curseforge),
}
impl Pack {
    pub fn inspect(bytes: &[u8], has_api_key: bool) -> Result<Self, Error> {
        if bytes.len() > MAX_PACK_BYTES {
            return Err("ModpackTooLarge".into());
        }
        if let Some(entry) = super::archive::find(bytes, "modrinth.index.json")? {
            return Ok(Self::Modrinth(Mrpack::parse(&super::archive::read(
                bytes,
                &entry,
                Mrpack::MAX_BYTES,
            )?)?));
        }
        if let Some(entry) = super::archive::find(bytes, "manifest.json")? {
            if !has_api_key {
                return Err("CurseforgeApiKeyRequired".into());
            }
            return Ok(Self::Curseforge(Curseforge::parse(&super::archive::read(
                bytes,
                &entry,
                Curseforge::MAX_BYTES,
            )?)?));
        }
        Err("UnknownModpackFormat".into())
    }
    pub fn identity(&self) -> Result<(String, String, Option<Selection>), Error> {
        match self {
            Self::Modrinth(p) => Ok((
                display_name(&p.name, p.version.as_deref()),
                p.dependencies.minecraft.clone(),
                p.dependencies.loader()?,
            )),
            Self::Curseforge(p) => Ok((
                display_name(&p.name, p.version.as_deref()),
                p.minecraft.version.clone(),
                p.minecraft.loader()?,
            )),
        }
    }
    pub fn override_directories(&self) -> Vec<&str> {
        match self {
            Self::Modrinth(_) => vec!["overrides", "client-overrides"],
            Self::Curseforge(p) => vec![p.overrides.as_deref().unwrap_or("overrides")],
        }
    }
}
fn display_name(name: &str, version: Option<&str>) -> String {
    version.map_or_else(|| name.into(), |v| format!("{name} {v}"))
}
fn kind(path: &str) -> Option<Kind> {
    [Kind::Mod, Kind::Resourcepack, Kind::Shaderpack]
        .into_iter()
        .find(|k| {
            path.strip_prefix(k.directory())
                .is_some_and(|s| s.starts_with('/') && s.len() > 1)
        })
}
fn usage(kind: Option<Kind>) -> Usage {
    Usage {
        mod_file: kind == Some(Kind::Mod),
        resourcepack: kind == Some(Kind::Resourcepack),
        shaderpack: kind == Some(Kind::Shaderpack),
        ..Usage::default()
    }
}
#[derive(Debug)]
pub struct Downloads {
    pub files: Vec<File>,
    pub entries: Vec<Entry>,
    pub skipped: usize,
}
impl Downloads {
    pub fn mrpack(pack: &Mrpack) -> Self {
        let mut result = Self {
            files: vec![],
            entries: vec![],
            skipped: 0,
        };
        for f in &pack.files {
            if !f.ships_to_client() {
                result.skipped += 1;
                continue;
            }
            let k = kind(&f.path);
            result.files.push(File {
                path: f.path.clone(),
                url: f.downloads[0].clone(),
                fallback_url: f.downloads.get(1).cloned(),
                sha1: f.hashes.sha1.clone(),
                size: f.size,
                usage: usage(k),
            });
            if let Some(kind) = k {
                let ids = schema::modrinth_ids(&f.downloads[0]);
                result.entries.push(Entry {
                    kind,
                    file: f.path.clone(),
                    sha1: f.hashes.sha1.clone(),
                    size: f.size,
                    source: Source {
                        provider: if ids.is_some() {
                            Provider::Modrinth
                        } else {
                            Provider::Manual
                        },
                        project_id: ids.map(|x| x.0.into()),
                        version_id: ids.map(|x| x.1.into()),
                        slug: None,
                    },
                    enabled: true,
                    as_dependency: false,
                })
            }
        }
        result
    }
    pub fn plan(&self, game_version: &str) -> Result<Plan, Error> {
        let total_download_size = self.files.iter().try_fold(0u64, |n, f| {
            n.checked_add(f.size)
                .ok_or(Error::from("DownloadSizeOverflow"))
        })?;
        Ok(Plan {
            version_id: game_version.into(),
            source: crate::metadata::Source::Official,
            target: Target::host().map_err(|e| Error(e.name().into()))?,
            files: self.files.clone(),
            classpath: vec![],
            native_extractions: vec![],
            asset_projections: vec![],
            total_download_size,
            java_major_version: 0,
            loader: "vanilla".into(),
        })
    }
}
/// Provider injection holds keys in the caller, not in plans, registry, or pack state.
pub trait CurseforgeProvider {
    fn project(&mut self, id: u32) -> Result<Project, Error>;
    fn file(&mut self, project: u32, file: u32) -> Result<ProviderFile, Error>;
    fn download_url(&mut self, project: u32, file: u32) -> Result<Option<String>, Error>;
}
pub struct Project {
    pub class_id: u32,
    pub slug: String,
}
pub struct ProviderFile {
    pub id: u32,
    pub project: u32,
    pub name: String,
    pub sha1: String,
    pub size: u64,
    pub url: Option<String>,
}
impl Downloads {
    pub fn curseforge(
        pack: &Curseforge,
        provider: &mut impl CurseforgeProvider,
    ) -> Result<Self, Error> {
        let mut result = Self {
            files: vec![],
            entries: vec![],
            skipped: 0,
        };
        let mut projects = std::collections::HashMap::new();
        let mut paths = std::collections::HashSet::new();
        for r in &pack.files {
            if !r.required {
                result.skipped += 1;
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(e) = projects.entry(r.project) {
                let p = provider.project(r.project)?;
                let k = match p.class_id {
                    6 => Kind::Mod,
                    12 => Kind::Resourcepack,
                    6552 => Kind::Shaderpack,
                    _ => return Err("UnsupportedProjectType".into()),
                };
                e.insert((k, p.slug));
            }
            let (kind, slug) = &projects[&r.project];
            let f = provider.file(r.project, r.file)?;
            if f.project != r.project {
                return Err("CurseforgeFileProjectMismatch".into());
            }
            let url = match f.url {
                Some(u) => Some(u),
                None => provider.download_url(r.project, f.id)?,
            }
            .ok_or(Error::from("CurseforgeDownloadUnavailable"))?;
            let path = format!("{}/{}", kind.directory(), f.name);
            if !paths.insert(path.clone()) {
                return Err("DuplicateModpackFile".into());
            }
            result.files.push(File {
                path: path.clone(),
                url,
                fallback_url: None,
                sha1: f.sha1.clone(),
                size: f.size,
                usage: usage(Some(*kind)),
            });
            result.entries.push(Entry {
                kind: *kind,
                file: path,
                sha1: f.sha1,
                size: f.size,
                source: Source {
                    provider: Provider::Curseforge,
                    project_id: Some(r.project.to_string()),
                    version_id: Some(r.file.to_string()),
                    slug: Some(slug.clone()),
                },
                enabled: true,
                as_dependency: false,
            });
        }
        Ok(result)
    }
}
/// Each override directory has independent count and byte budgets, as in Zig.
/// Callback must atomically replace a private mutable file; never write through a CAS hardlink.
pub fn overrides_bytes(
    bytes: &[u8],
    directory: &str,
    mut write: impl FnMut(&[u8], &[u8]) -> Result<(), Error>,
) -> Result<usize, Error> {
    let prefix = format!("{directory}/");
    let mut it = super::archive::Entries::new(bytes)?;
    let mut count = 0;
    let mut total = 0u64;
    while let Some(e) = it.next_entry()? {
        let Some(relative) = e.name.strip_prefix(prefix.as_bytes()) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        schema::relative_path_bytes(relative)?;
        if relative.len() > 512 {
            return Err("InvalidModpackOverride".into());
        }
        if count >= 8192 {
            return Err("TooManyModpackOverrides".into());
        }
        let data = super::archive::read(bytes, &e, 64 * 1024 * 1024)?;
        total = total
            .checked_add(data.len() as u64)
            .ok_or(Error::from("ModpackOverridesTooLarge"))?;
        if total > 512 * 1024 * 1024 {
            return Err("ModpackOverridesTooLarge".into());
        }
        write(relative, &data)?;
        count += 1;
    }
    Ok(count)
}

pub fn overrides(
    bytes: &[u8],
    directory: &str,
    mut write: impl FnMut(&str, &[u8]) -> Result<(), Error>,
) -> Result<usize, Error> {
    overrides_bytes(bytes, directory, |path, data| {
        write(
            std::str::from_utf8(path).map_err(|_| Error::from("UnsafePath"))?,
            data,
        )
    })
}

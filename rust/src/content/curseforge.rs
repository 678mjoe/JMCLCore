//! Caller-keyed CurseForge resolution; keys never enter returned metadata/plans.
use super::{
    modrinth as mr,
    registry::{Kind, Provider, Registry},
    remote::{Resolution, Resolved},
};
use crate::modpack::provider;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::HashSet;
pub trait Metadata {
    fn get(&self, url: &str, key: &str, limit: usize) -> mr::Result<Vec<u8>>;
}
fn map(e: crate::modpack::Error) -> mr::Error {
    mr::Error(e.name().into())
}
pub fn id(s: &str) -> mr::Result<()> {
    if s.is_empty()
        || s.len() > 10
        || !s.bytes().all(|b| b.is_ascii_digit())
        || s.parse::<u32>().ok().is_none_or(|n| n == 0)
    {
        Err("InvalidCurseforgeId".into())
    } else {
        Ok(())
    }
}
fn number32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    mr::number_width(d, 32).map(|n| n as u32)
}
fn number8<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    mr::number_width(d, 8).map(|n| n as u8)
}
#[derive(Deserialize)]
struct Envelope {
    data: Box<RawValue>,
    pagination: Option<Pagination>,
}
#[derive(Deserialize)]
#[allow(dead_code)] // Fields are parsed for original schema validation.
struct Pagination {
    #[serde(default, deserialize_with = "number32")]
    index: u32,
    #[serde(default, rename = "pageSize", deserialize_with = "number32")]
    page_size: u32,
    #[serde(default, rename = "resultCount", deserialize_with = "number32")]
    result_count: u32,
    #[serde(default, rename = "totalCount", deserialize_with = "number32")]
    total: u32,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct Project {
    #[serde(deserialize_with = "number32")]
    id: u32,
    #[serde(deserialize_with = "mr::string")]
    slug: String,
    #[serde(deserialize_with = "mr::string")]
    name: String,
    #[serde(rename = "classId", deserialize_with = "number32")]
    class_id: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
}
fn parse_project(bytes: &[u8]) -> mr::Result<Project> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let env: Envelope = mr::parse(bytes, 2 * 1024 * 1024, false)?;
    let p: Project = mr::parse(env.data.get().as_bytes(), 2 * 1024 * 1024, false)?;
    if p.id == 0 {
        return Err("InvalidCurseforgeId".into());
    }
    if p.slug.is_empty()
        || p.slug.len() > 128
        || !p
            .slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err("InvalidCurseforgeSlug".into());
    }
    Ok(p)
}
pub(super) fn download_url(bytes: &[u8]) -> mr::Result<Option<String>> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let _: Envelope = mr::parse(bytes, 2 * 1024 * 1024, false)?;
    #[derive(Deserialize)]
    struct Url {
        #[serde(deserialize_with = "mr::optional_string")]
        data: Option<String>,
    }
    let u: Url = mr::parse(bytes, 2 * 1024 * 1024, false)?;
    if let Some(u) = &u.data {
        validate_url(u)?
    }
    Ok(u.data)
}
fn validate_url(u: &str) -> mr::Result<()> {
    if !u.starts_with("https://") {
        return Err("InsecureCurseforgeUrl".into());
    }
    if u.len() > 2048
        || u.bytes()
            .any(|b| b <= 32 || b == 127 || b == b'\\' || b == b'"')
    {
        return Err("InvalidCurseforgeUrl".into());
    }
    Ok(())
}

#[derive(Clone, Deserialize)]
pub struct Dependency {
    #[serde(rename = "modId", deserialize_with = "number32")]
    pub id: u32,
    #[serde(rename = "relationType", deserialize_with = "number8")]
    pub relation: u8,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
struct Hash {
    #[serde(deserialize_with = "mr::string")]
    value: String,
    #[serde(deserialize_with = "number8")]
    algo: u8,
}
#[derive(Deserialize)]
struct Detail {
    #[serde(deserialize_with = "number32")]
    id: u32,
    #[serde(rename = "modId", deserialize_with = "number32")]
    project: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
    #[serde(rename = "fileName", deserialize_with = "mr::string")]
    name: String,
    #[serde(rename = "releaseType", deserialize_with = "number8")]
    release: u8,
    hashes: Vec<Hash>,
    #[serde(rename = "fileDate", deserialize_with = "mr::string")]
    date: String,
    #[serde(rename = "fileLength", deserialize_with = "mr::number")]
    size: u64,
    #[serde(
        default,
        rename = "downloadUrl",
        deserialize_with = "mr::optional_string"
    )]
    url: Option<String>,
    #[serde(rename = "gameVersions", deserialize_with = "mr::strings")]
    games: Vec<String>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}
#[derive(Clone)]
pub struct File {
    pub id: u32,
    pub project: u32,
    pub name: String,
    pub sha1: String,
    pub size: u64,
    pub url: Option<String>,
    pub available: bool,
    pub release: u8,
    pub date: String,
    pub games: Vec<String>,
    pub dependencies: Vec<Dependency>,
}
pub fn parse_file(bytes: &[u8]) -> mr::Result<File> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let env: Envelope = mr::parse(bytes, 2 * 1024 * 1024, false)?;
    parse_file_data(env.data.get().as_bytes(), 2 * 1024 * 1024)
}
fn parse_file_data(bytes: &[u8], max: usize) -> mr::Result<File> {
    let d: Detail = mr::parse(bytes, max, false)?;
    if d.id == 0 || d.project == 0 {
        return Err("InvalidCurseforgeId".into());
    }
    let n = &d.name;
    let stem = n.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if n.is_empty()
        || n.len() > 256
        || n == "."
        || n == ".."
        || n.ends_with(['.', ' '])
        || n.bytes()
            .any(|b| b < 32 || b == 127 || b"/\\:".contains(&b))
        || reserved
    {
        return Err("InvalidCurseforgeFilename".into());
    }
    if !(1..=3).contains(&d.release) {
        return Err("InvalidCurseforgeReleaseType".into());
    }
    if d.games.is_empty()
        || d.games.len() > 128
        || d.games.iter().any(|s| s.is_empty() || s.len() > 64)
    {
        return Err("InvalidCurseforgeGameVersions".into());
    }
    if d.hashes.len() > 8 {
        return Err("InvalidCurseforgeHash".into());
    }
    let mut sha1 = None;
    for h in d.hashes {
        if h.algo == 1 {
            if h.value.len() != 40 || !h.value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("InvalidCurseforgeHash".into());
            }
            if sha1.is_none() {
                sha1 = Some(h.value)
            }
        }
    }
    let sha1 = sha1.ok_or_else(|| mr::Error::from("MissingCurseforgeSha1"))?;
    if d.size == 0 || d.size > 512 * 1024 * 1024 {
        return Err("InvalidCurseforgeFileSize".into());
    }
    if let Some(u) = &d.url {
        if !u.starts_with("https://") {
            return Err("InsecureCurseforgeUrl".into());
        }
        if u.len() > 2048
            || u.bytes()
                .any(|b| b <= 32 || b == 127 || b == b'\\' || b == b'"')
        {
            return Err("InvalidCurseforgeUrl".into());
        }
    }
    if d.dependencies.len() > 128 {
        return Err("TooManyCurseforgeDependencies".into());
    }
    Ok(File {
        id: d.id,
        project: d.project,
        name: d.name,
        sha1,
        size: d.size,
        url: d.url,
        available: d.available,
        release: d.release,
        date: d.date,
        games: d.games,
        dependencies: d.dependencies,
    })
}

pub fn supports(f: &File, game: &str, loader: Option<&str>) -> bool {
    let loader = loader.filter(|l| matches!(*l, "fabric" | "forge" | "neoforge"));
    f.games.iter().any(|s| s == game)
        && loader.is_none_or(|l| f.games.iter().any(|s| s.eq_ignore_ascii_case(l)))
}
pub fn select<'a>(files: &'a [File], ext: Option<&str>) -> Option<&'a File> {
    let mut best: Option<&File> = None;
    for f in files {
        if !f.available || ext.is_some_and(|e| !f.name.ends_with(e)) {
            continue;
        }
        if best.is_none_or(|b| f.release < b.release || (f.release == b.release && f.date > b.date))
        {
            best = Some(f)
        }
    }
    best
}
fn url(project: &str) -> mr::Result<String> {
    id(project)?;
    Ok(format!("https://api.curseforge.com/v1/mods/{project}"))
}
pub(super) fn file_url(project: &str, file: &str) -> mr::Result<String> {
    id(file)?;
    Ok(format!("{}/files/{file}", url(project)?))
}
pub fn page_url(project: &str, game: &str, loader: Option<&str>, index: u32) -> mr::Result<String> {
    id(project)?;
    mr::game_version(game)?;
    let mut u = format!(
        "{}/files?gameVersion={}",
        url(project)?,
        game.replace('+', "%2B")
    );
    if let Some(l) = loader {
        let n = match l {
            "forge" => 1,
            "fabric" => 4,
            "neoforge" => 6,
            _ => return Err("InvalidContentLoader".into()),
        };
        u.push_str(&format!("&modLoaderType={n}"))
    }
    u.push_str(&format!("&index={index}&pageSize=50"));
    Ok(u)
}
struct Resolver<'a, T> {
    transport: &'a T,
    key: &'a str,
    kind: Kind,
    game: &'a str,
    loader: Option<&'a str>,
    dependencies: bool,
    registry: &'a Registry,
    seen: HashSet<String>,
    result: Resolution,
}
impl<T: Metadata> Resolver<'_, T> {
    fn get(&self, url: &str, max: usize) -> mr::Result<Vec<u8>> {
        let b = self.transport.get(url, self.key, max)?;
        if b.len() > max {
            return Err("CurseforgeResponseTooLarge".into());
        }
        Ok(b)
    }
    fn pages(&self, project: &str) -> mr::Result<Vec<File>> {
        let mut files = vec![];
        let mut index = 0;
        loop {
            let bytes = self.get(
                &page_url(project, self.game, self.loader, index)?,
                8 * 1024 * 1024,
            )?;
            let e: Envelope = mr::parse(&bytes, 8 * 1024 * 1024, false)?;
            let raw: Vec<Box<RawValue>> =
                mr::parse(e.data.get().as_bytes(), 8 * 1024 * 1024, true)?;
            if raw.len() > 50 {
                return Err("TooManyCurseforgeFiles".into());
            }
            let count = raw.len();
            for f in raw {
                files.push(parse_file_data(f.get().as_bytes(), 8 * 1024 * 1024)?)
            }
            if files.len() > 512 {
                return Err("TooManyCurseforgeFiles".into());
            }
            if count == 0
                || e.pagination
                    .as_ref()
                    .is_none_or(|p| files.len() >= p.total as usize)
                || count < 50
            {
                break;
            }
            index += count as u32;
        }
        Ok(files)
    }
    fn resolve(
        &mut self,
        project: &str,
        file: Option<&str>,
        dependency: bool,
        depth: u8,
    ) -> mr::Result<()> {
        if depth > 32 {
            return Err("ContentDependencyTooDeep".into());
        }
        if self.result.files.len() >= 256 {
            return Err("TooManyContentDependencies".into());
        }
        let bytes = self.get(&url(project)?, 2 * 1024 * 1024)?;
        let p = parse_project(&bytes)?;
        let kind = match p.class_id {
            6 => Kind::Mod,
            12 => Kind::Resourcepack,
            6552 => Kind::Shaderpack,
            5290 => Kind::Datapack,
            _ => return Err("UnsupportedProjectType".into()),
        };
        if kind != self.kind {
            return Err("ContentKindMismatch".into());
        }
        let project = p.id.to_string();
        if !self.seen.insert(project.clone()) {
            return Ok(());
        }
        if self.registry.entries.iter().any(|e| {
            e.source.provider == Provider::Curseforge
                && e.source.project_id.as_deref() == Some(&project)
        }) {
            self.result.skipped += 1;
            return Ok(());
        }
        let f = if let Some(id) = file {
            let f = parse_file(&self.get(&file_url(&project, id)?, 2 * 1024 * 1024)?)?;
            if f.project != p.id {
                return Err("CurseforgeFileProjectMismatch".into());
            }
            if !supports(&f, self.game, self.loader) {
                return Err("IncompatibleContentVersion".into());
            }
            if !f.available {
                return Err("CurseforgeDownloadUnavailable".into());
            }
            if self.kind == Kind::Datapack && !f.name.ends_with(".zip") {
                return Err("ContentFileSelectionFailed".into());
            }
            f
        } else {
            let files = self.pages(&project)?;
            select(
                &files,
                if self.kind == Kind::Datapack {
                    Some(".zip")
                } else {
                    None
                },
            )
            .ok_or_else(|| mr::Error::from("NoCompatibleContentVersion"))?
            .clone()
        };
        let url = match f.url {
            Some(u) => u,
            None => download_url(&self.get(
                &format!("{}/download-url", file_url(&project, &f.id.to_string())?),
                2 * 1024 * 1024,
            )?)?
            .ok_or_else(|| mr::Error::from("CurseforgeDownloadUnavailable"))?,
        };
        self.result.files.push(Resolved {
            project_id: project,
            slug: Some(p.slug),
            version_id: f.id.to_string(),
            file_name: f.name,
            url,
            sha1: f.sha1,
            size: f.size,
            as_dependency: dependency,
        });
        if self.dependencies {
            for d in f.dependencies {
                if d.relation == 3 {
                    self.resolve(&d.id.to_string(), None, true, depth + 1)?
                }
            }
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn resolve<T: Metadata>(
    transport: &T,
    kind: Kind,
    game: &str,
    loader: Option<&str>,
    key: &str,
    project: &str,
    file: Option<&str>,
    dependencies: bool,
    registry: &Registry,
) -> mr::Result<Resolution> {
    id(project)?;
    provider::validate_key(key).map_err(map)?;
    mr::game_version(game)?;
    if let Some(f) = file {
        id(f)?
    }
    match kind {
        Kind::Mod => {
            let l = loader.ok_or_else(|| mr::Error::from("ModLoaderRequired"))?;
            if !matches!(l, "fabric" | "forge" | "neoforge") {
                return Err("InvalidContentLoader".into());
            }
        }
        _ => {
            if loader.is_some() {
                return Err("InvalidContentLoader".into());
            }
        }
    }
    let mut r = Resolver {
        transport,
        key,
        kind,
        game,
        loader,
        dependencies,
        registry,
        seen: HashSet::new(),
        result: Resolution {
            files: vec![],
            skipped: 0,
        },
    };
    r.resolve(project, file, false, 0)?;
    Ok(r.result)
}

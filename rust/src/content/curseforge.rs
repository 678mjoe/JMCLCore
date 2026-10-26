//! Caller-keyed CurseForge resolution; keys never enter returned metadata/plans.
#[path = "curseforge_file.rs"]
mod file_ordered;
use super::{
    modrinth as mr,
    registry::{Kind, Provider, Registry},
    remote::Resolution,
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
    #[allow(dead_code)] // Retained raw schema check for download_url.
    data: Box<RawValue>,
    #[allow(dead_code)] // Still typed for single-record schema/error compatibility.
    pagination: Option<Pagination>,
}
#[derive(Deserialize)]
struct TypedEnvelope<T> {
    data: T,
    #[allow(dead_code)] // Typed in input order even when only data is consumed.
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
    name: mr::MetadataBytes,
    #[serde(rename = "classId", deserialize_with = "number32")]
    class_id: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
}
// Only call after BOTH envelope and inner typed schema succeed, never after
// Envelope<RawValue> alone. Earlier schema errors must retain their precedence.
fn validate_completed_document(bytes: &[u8]) -> mr::Result<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| mr::Error::from("SyntaxError"))?;
    crate::auth::json::Json::parse(text)
        .map(|_| ())
        .map_err(|_| mr::Error::from("SyntaxError"))
}
fn parse_project(bytes: &[u8]) -> mr::Result<Project> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let env: TypedEnvelope<Project> = mr::parse(bytes, 2 * 1024 * 1024, false)?;
    let p = env.data;
    validate_completed_document(bytes)?;
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
    value: mr::MetadataBytes,
    #[serde(deserialize_with = "number8")]
    algo: u8,
}
#[derive(Deserialize)]
#[serde(bound(deserialize = "F: mr::FilenameValue"))]
struct Detail<F> {
    #[serde(deserialize_with = "number32")]
    id: u32,
    #[serde(rename = "modId", deserialize_with = "number32")]
    project: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
    #[serde(rename = "fileName", deserialize_with = "mr::filename_value")]
    name: F,
    #[serde(rename = "releaseType", deserialize_with = "number8")]
    release: u8,
    hashes: Vec<Hash>,
    #[serde(rename = "fileDate")]
    date: mr::MetadataBytes,
    #[serde(rename = "fileLength", deserialize_with = "mr::number")]
    size: u64,
    #[serde(
        default,
        rename = "downloadUrl",
        deserialize_with = "mr::optional_string"
    )]
    url: Option<String>,
    #[serde(rename = "gameVersions")]
    games: Vec<mr::MetadataBytes>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}
#[derive(Clone)]
pub struct FileRecord<F> {
    pub id: u32,
    pub project: u32,
    pub name: F,
    pub sha1: String,
    pub size: u64,
    pub url: Option<String>,
    pub available: bool,
    pub release: u8,
    pub date: mr::MetadataBytes,
    pub games: Vec<mr::MetadataBytes>,
    pub dependencies: Vec<Dependency>,
}
pub(super) fn parse_file_as<F: mr::FilenameValue>(bytes: &[u8]) -> mr::Result<FileRecord<F>> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let detail = file_ordered::parse(bytes)?;
    validate_detail_as(detail)
}
fn validate_detail_as<F: mr::FilenameValue>(d: Detail<F>) -> mr::Result<FileRecord<F>> {
    if d.id == 0 || d.project == 0 {
        return Err("InvalidCurseforgeId".into());
    }
    let n = d.name.filename_bytes();
    let stem = n
        .split(|b| *b == b'.')
        .next()
        .unwrap_or(b"")
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_slice(), b"CON" | b"PRN" | b"AUX" | b"NUL")
        || ((stem.starts_with(b"COM") || stem.starts_with(b"LPT"))
            && stem.len() == 4
            && matches!(stem[3], b'1'..=b'9'));
    if n.is_empty()
        || n.len() > 256
        || n == b"."
        || n == b".."
        || n.last().is_some_and(|b| matches!(b, b'.' | b' '))
        || n.iter()
            .any(|b| *b < 32 || *b == 127 || b"/\\:".contains(b))
        || reserved
    {
        return Err("InvalidCurseforgeFilename".into());
    }
    if !(1..=3).contains(&d.release) {
        return Err("InvalidCurseforgeReleaseType".into());
    }
    if d.games.is_empty()
        || d.games.len() > 128
        || d.games
            .iter()
            .any(|s| s.as_bytes().is_empty() || s.as_bytes().len() > 64)
    {
        return Err("InvalidCurseforgeGameVersions".into());
    }
    if d.hashes.len() > 8 {
        return Err("InvalidCurseforgeHash".into());
    }
    let mut sha1 = None;
    for h in d.hashes {
        if h.algo == 1 {
            if h.value.as_bytes().len() != 40
                || !h.value.as_bytes().iter().all(u8::is_ascii_hexdigit)
            {
                return Err("InvalidCurseforgeHash".into());
            }
            if sha1.is_none() {
                // Conversion is safe only AFTER exact ASCII-hex integrity validation.
                sha1 = Some(String::from_utf8(h.value.into_bytes()).expect("validated ASCII SHA1"))
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
    Ok(FileRecord {
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

pub fn supports<F>(f: &FileRecord<F>, game: &str, loader: Option<&str>) -> bool {
    let loader = loader.filter(|l| matches!(*l, "fabric" | "forge" | "neoforge"));
    f.games.iter().any(|s| s.as_bytes() == game.as_bytes())
        && loader.is_none_or(|l| {
            f.games
                .iter()
                .any(|s| s.as_bytes().eq_ignore_ascii_case(l.as_bytes()))
        })
}
fn select_as<'a, F: mr::FilenameValue>(
    files: &'a [FileRecord<F>],
    ext: Option<&str>,
) -> Option<&'a FileRecord<F>> {
    let mut best: Option<&FileRecord<F>> = None;
    for f in files {
        if !f.available || ext.is_some_and(|e| !f.name.filename_bytes().ends_with(e.as_bytes())) {
            continue;
        }
        if best.is_none_or(|b| {
            f.release < b.release
                || (f.release == b.release && f.date.as_bytes() > b.date.as_bytes())
        }) {
            best = Some(f)
        }
    }
    best
}
pub type File = FileRecord<String>;
pub fn parse_file(bytes: &[u8]) -> mr::Result<File> {
    parse_file_as(bytes)
}

pub fn select<'a>(files: &'a [File], ext: Option<&str>) -> Option<&'a File> {
    select_as(files, ext)
}
#[cfg(test)]
fn parse_page(bytes: &[u8]) -> mr::Result<(Vec<File>, Option<Pagination>)> {
    parse_page_as(bytes)
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
// The source types the entire page before length and per-file semantic checks.
#[derive(Deserialize)]
#[serde(bound(deserialize = "F: mr::FilenameValue"))]
struct PageEnvelope<F> {
    data: Vec<Detail<F>>,
    pagination: Option<Pagination>,
}
fn parse_page_as<F: mr::FilenameValue>(
    bytes: &[u8],
) -> mr::Result<(Vec<FileRecord<F>>, Option<Pagination>)> {
    const LIMIT: usize = 8 * 1024 * 1024;
    if bytes.len() > LIMIT {
        return Err("CurseforgeResponseTooLarge".into());
    }
    let page: PageEnvelope<F> = mr::parse(bytes, LIMIT, false)?;
    validate_completed_document(bytes)?;
    if page.data.len() > 50 {
        return Err("TooManyCurseforgeFiles".into());
    }
    let files = page
        .data
        .into_iter()
        .map(validate_detail_as)
        .collect::<mr::Result<_>>()?;
    Ok((files, page.pagination))
}
struct Resolver<'a, T, F, R: super::registry::RegistryFile> {
    transport: &'a T,
    key: &'a str,
    kind: Kind,
    game: &'a str,
    loader: Option<&'a str>,
    dependencies: bool,
    registry: &'a super::registry::RegistryRecord<R>,
    seen: HashSet<String>,
    result: super::remote::ResolutionRecord<F>,
}
impl<T: Metadata, F: mr::FilenameValue + Clone, R: super::registry::RegistryFile>
    Resolver<'_, T, F, R>
{
    fn get(&self, url: &str, max: usize) -> mr::Result<Vec<u8>> {
        let b = self.transport.get(url, self.key, max)?;
        if b.len() > max {
            return Err("CurseforgeResponseTooLarge".into());
        }
        Ok(b)
    }
    fn pages(&self, project: &str) -> mr::Result<Vec<FileRecord<F>>> {
        let mut files = vec![];
        let mut index = 0;
        loop {
            let bytes = self.get(
                &page_url(project, self.game, self.loader, index)?,
                8 * 1024 * 1024,
            )?;
            let (page, pagination) = parse_page_as(&bytes)?;
            let count = page.len();
            files.extend(page);
            if files.len() > 512 {
                return Err("TooManyCurseforgeFiles".into());
            }
            if count == 0
                || pagination
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
            let f = parse_file_as::<F>(&self.get(&file_url(&project, id)?, 2 * 1024 * 1024)?)?;
            if f.project != p.id {
                return Err("CurseforgeFileProjectMismatch".into());
            }
            if !supports(&f, self.game, self.loader) {
                return Err("IncompatibleContentVersion".into());
            }
            if !f.available {
                return Err("CurseforgeDownloadUnavailable".into());
            }
            if self.kind == Kind::Datapack && !f.name.filename_bytes().ends_with(b".zip") {
                return Err("ContentFileSelectionFailed".into());
            }
            f
        } else {
            let files = self.pages(&project)?;
            select_as(
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
        self.result.files.push(super::remote::ResolvedRecord {
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
fn resolve_as<T: Metadata, F: mr::FilenameValue + Clone, R: super::registry::RegistryFile>(
    transport: &T,
    kind: Kind,
    game: &str,
    loader: Option<&str>,
    key: &str,
    project: &str,
    file: Option<&str>,
    dependencies: bool,
    registry: &super::registry::RegistryRecord<R>,
) -> mr::Result<super::remote::ResolutionRecord<F>> {
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
        result: super::remote::ResolutionRecord {
            files: vec![],
            skipped: 0,
        },
    };
    r.resolve(project, file, false, 0)?;
    Ok(r.result)
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
    resolve_as(
        transport,
        kind,
        game,
        loader,
        key,
        project,
        file,
        dependencies,
        registry,
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_bytes<T: Metadata>(
    transport: &T,
    kind: Kind,
    game: &str,
    loader: Option<&str>,
    key: &str,
    project: &str,
    file: Option<&str>,
    dependencies: bool,
    registry: &super::registry::ListingRegistry,
) -> mr::Result<super::remote::ByteResolution> {
    resolve_as(
        transport,
        kind,
        game,
        loader,
        key,
        project,
        file,
        dependencies,
        registry,
    )
}
#[cfg(test)]
#[path = "../../tests/support/curseforge_metadata_bytes.rs"]
mod byte_metadata_tests;

#[cfg(test)]
#[path = "../../tests/support/curseforge_lexical_acceptance.rs"]
mod lexical_acceptance_tests;

#[cfg(test)]
#[path = "../../tests/support/curseforge_page_parser.rs"]
mod page_parser_tests;

#[cfg(test)]
#[path = "../../tests/support/curseforge_typed_envelope.rs"]
mod typed_envelope_tests;

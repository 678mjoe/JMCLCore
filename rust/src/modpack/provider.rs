//! Bounded CurseForge response decoding. Secrets remain caller-owned and are
//! never copied into errors, plans, manifests, registry entries, or events.
#![allow(dead_code)] // Retained fields enforce the original typed schema.
use super::{
    Error,
    planning::{CurseforgeProvider, Project, ProviderFile},
    schema::{Object, object_vec, optional_object, parse, u8_number, u32_number, u64_number},
};
use serde::Deserialize;
use std::io::Read;
const MAX: usize = 2 * 1024 * 1024;
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
struct Pagination {
    #[serde(default, deserialize_with = "u32_number")]
    index: u32,
    #[serde(default, rename = "pageSize", deserialize_with = "u32_number")]
    page_size: u32,
    #[serde(default, rename = "resultCount", deserialize_with = "u32_number")]
    result_count: u32,
    #[serde(default, rename = "totalCount", deserialize_with = "u32_number")]
    total_count: u32,
}
#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Envelope<T> {
    #[serde(deserialize_with = "required")]
    data: T,
    #[serde(default, deserialize_with = "optional_object")]
    pagination: Option<Pagination>,
}
#[derive(Deserialize)]
struct Mod {
    #[serde(deserialize_with = "u32_number")]
    id: u32,
    slug: String,
    name: String,
    #[serde(rename = "classId", deserialize_with = "u32_number")]
    class: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
}
#[derive(Deserialize)]
struct Hash {
    value: String,
    #[serde(deserialize_with = "u8_number")]
    algo: u8,
}
#[derive(Deserialize)]
struct Dependency {
    #[serde(rename = "modId", deserialize_with = "u32_number")]
    id: u32,
    #[serde(rename = "relationType", deserialize_with = "u8_number")]
    kind: u8,
}
#[derive(Deserialize)]
struct File {
    #[serde(deserialize_with = "u32_number")]
    id: u32,
    #[serde(rename = "modId", deserialize_with = "u32_number")]
    project: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
    #[serde(rename = "fileName")]
    name: String,
    #[serde(rename = "releaseType", deserialize_with = "u8_number")]
    release: u8,
    #[serde(deserialize_with = "object_vec")]
    hashes: Vec<Hash>,
    #[serde(rename = "fileDate")]
    date: String,
    #[serde(rename = "fileLength", deserialize_with = "u64_number")]
    size: u64,
    #[serde(rename = "downloadUrl")]
    url: Option<String>,
    #[serde(rename = "gameVersions")]
    games: Vec<String>,
    #[serde(default, deserialize_with = "object_vec")]
    dependencies: Vec<Dependency>,
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > MAX {
        return Err("CurseforgeResponseTooLarge".into());
    }
    parse(bytes)
}
pub fn validate_key(key: &str) -> Result<(), Error> {
    if key.is_empty() || key.len() > 512 || key.bytes().any(|b| !(0x21..=0x7e).contains(&b)) {
        Err("InvalidCurseforgeApiKey".into())
    } else {
        Ok(())
    }
}
fn url(s: &str) -> Result<(), Error> {
    if !s.starts_with("https://") {
        return Err("InsecureCurseforgeUrl".into());
    }
    if s.len() > 2048
        || s.bytes()
            .any(|b| b <= 32 || b == 127 || b == b'\\' || b == b'"')
    {
        return Err("InvalidCurseforgeUrl".into());
    }
    Ok(())
}
pub fn project(bytes: &[u8]) -> Result<Project, Error> {
    let e: Envelope<Object<Mod>> = decode(bytes)?;
    let p = e.data.0;
    if p.id == 0 {
        return Err("InvalidCurseforgeId".into());
    }
    if p.slug.is_empty()
        || p.slug.len() > 128
        || p.slug
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && !b"-_.".contains(&b))
    {
        return Err("InvalidCurseforgeSlug".into());
    }
    Ok(Project {
        class_id: p.class,
        slug: p.slug,
    })
}
pub fn file(bytes: &[u8]) -> Result<ProviderFile, Error> {
    let e: Envelope<Object<File>> = decode(bytes)?;
    let f = e.data.0;
    if f.id == 0 || f.project == 0 {
        return Err("InvalidCurseforgeId".into());
    }
    if f.name.len() > 256 || f.name.contains('/') || super::schema::relative_path(&f.name).is_err()
    {
        return Err("InvalidCurseforgeFilename".into());
    }
    if !(1..=3).contains(&f.release) {
        return Err("InvalidCurseforgeReleaseType".into());
    }
    if f.games.is_empty()
        || f.games.len() > 128
        || f.games.iter().any(|s| s.is_empty() || s.len() > 64)
    {
        return Err("InvalidCurseforgeGameVersions".into());
    }
    if f.hashes.len() > 8 {
        return Err("InvalidCurseforgeHash".into());
    }
    let mut sha1 = None;
    for hash in f.hashes {
        if hash.algo == 1 {
            if hash.value.len() != 40 || !hash.value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("InvalidCurseforgeHash".into());
            }
            if sha1.is_none() {
                sha1 = Some(hash.value)
            }
        }
    }
    let sha1 = sha1.ok_or(Error::from("MissingCurseforgeSha1"))?;
    if f.size == 0 || f.size > 512 * 1024 * 1024 {
        return Err("InvalidCurseforgeFileSize".into());
    }
    if let Some(s) = &f.url {
        url(s)?
    }
    if f.dependencies.len() > 128 {
        return Err("TooManyCurseforgeDependencies".into());
    }
    Ok(ProviderFile {
        id: f.id,
        project: f.project,
        name: f.name,
        sha1,
        size: f.size,
        url: f.url,
    })
}
pub fn download_url(bytes: &[u8]) -> Result<Option<String>, Error> {
    let e: Envelope<Option<String>> = decode(bytes)?;
    if let Some(s) = &e.data {
        url(s)?
    }
    Ok(e.data)
}
/// API response transport is separate from artifact transport; the key must not
/// be forwarded to CDN artifact downloads or cross-origin redirect targets.
pub trait ApiTransport {
    fn get(&mut self, path: &str, key: &str) -> Result<Vec<u8>, Error>;
}
pub struct Provider<'a, T> {
    pub transport: T,
    pub key: &'a str,
}
impl<T: ApiTransport> CurseforgeProvider for Provider<'_, T> {
    fn project(&mut self, id: u32) -> Result<Project, Error> {
        project(&self.transport.get(&format!("/v1/mods/{id}"), self.key)?)
    }
    fn file(&mut self, project: u32, id: u32) -> Result<ProviderFile, Error> {
        file(
            &self
                .transport
                .get(&format!("/v1/mods/{project}/files/{id}"), self.key)?,
        )
    }
    fn download_url(&mut self, project: u32, id: u32) -> Result<Option<String>, Error> {
        download_url(&self.transport.get(
            &format!("/v1/mods/{project}/files/{id}/download-url"),
            self.key,
        )?)
    }
}
pub struct HttpApi(reqwest::blocking::Client);
impl HttpApi {
    pub fn new() -> Result<Self, Error> {
        Ok(Self(
            reqwest::blocking::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() > 3 {
                        return attempt.error("TooManyHttpRedirects");
                    }
                    if attempt.url().scheme() != "https"
                        || attempt.url().host_str() != Some("api.curseforge.com")
                        || attempt.url().port_or_known_default() != Some(443)
                    {
                        return attempt.error("UnsafeCredentialRedirect");
                    }
                    attempt.follow()
                }))
                .timeout(std::time::Duration::from_secs(60))
                .user_agent("jmcl-core/0.1.0-dev")
                .build()
                .map_err(|_| Error::from("TlsInitializationFailed"))?,
        ))
    }
}
impl ApiTransport for HttpApi {
    fn get(&mut self, path: &str, key: &str) -> Result<Vec<u8>, Error> {
        validate_key(key)?;
        let response = self
            .0
            .get(format!("https://api.curseforge.com{path}"))
            .header("x-api-key", key)
            .send()
            .map_err(|e| {
                Error::from(if e.is_timeout() {
                    "ConnectionTimedOut"
                } else if e.is_redirect() {
                    "TooManyHttpRedirects"
                } else {
                    "HttpRequestFailed"
                })
            })?;
        let success = response.status().is_success();
        let mut bytes = Vec::new();
        response
            .take(MAX as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::from("ReadFailed"))?;
        if bytes.len() > MAX {
            return Err("ResponseBodyTooLarge".into());
        }
        if !success {
            return Err("UnsuccessfulHttpStatus".into());
        }
        Ok(bytes)
    }
}

fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    T::deserialize(d)
}

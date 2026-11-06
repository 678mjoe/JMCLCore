//! Bounded CurseForge response decoding. Secrets remain caller-owned and are
//! never copied into errors, plans, manifests, registry entries, or events.
#![allow(dead_code)] // Retained fields enforce the original typed schema.
use super::{
    Error,
    planning::{CurseforgeProvider, Project, ProviderFile},
    schema::{Object, object_vec, optional_object, parse, typed_u8_number, u32_number, u64_number},
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
    slug: crate::content::modrinth::MetadataBytes,
    name: crate::content::modrinth::MetadataBytes,
    #[serde(rename = "classId", deserialize_with = "u32_number")]
    class: u32,
    #[serde(default = "yes", rename = "isAvailable")]
    available: bool,
}
#[derive(Deserialize)]
struct Hash {
    value: crate::content::modrinth::MetadataBytes,
    #[serde(deserialize_with = "typed_u8_number")]
    algo: u8,
}
#[derive(Deserialize)]
struct Dependency {
    #[serde(rename = "modId", deserialize_with = "u32_number")]
    id: u32,
    #[serde(rename = "relationType", deserialize_with = "typed_u8_number")]
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
    name: crate::content::modrinth::MetadataBytes,
    #[serde(rename = "releaseType", deserialize_with = "typed_u8_number")]
    release: u8,
    #[serde(deserialize_with = "object_vec")]
    hashes: Vec<Hash>,
    #[serde(rename = "fileDate")]
    date: crate::content::modrinth::MetadataBytes,
    #[serde(rename = "fileLength", deserialize_with = "u64_number")]
    size: u64,
    #[serde(rename = "downloadUrl")]
    url: Option<String>,
    #[serde(rename = "gameVersions")]
    games: Vec<crate::content::modrinth::MetadataBytes>,
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
    if p.slug.as_bytes().is_empty()
        || p.slug.as_bytes().len() > 128
        || p.slug
            .as_bytes()
            .iter()
            .any(|b| !b.is_ascii_alphanumeric() && !b"-_.".contains(b))
    {
        return Err("InvalidCurseforgeSlug".into());
    }
    Ok(Project {
        class_id: p.class,
        slug: String::from_utf8(p.slug.into_bytes()).expect("validated ASCII slug"),
    })
}
pub fn file(bytes: &[u8]) -> Result<ProviderFile, Error> {
    let e: Envelope<Object<File>> = decode(bytes)?;
    let f = e.data.0;
    if f.id == 0 || f.project == 0 {
        return Err("InvalidCurseforgeId".into());
    }
    if f.name.as_bytes().len() > 256
        || f.name.as_bytes().contains(&b'/')
        || super::schema::relative_path_bytes(f.name.as_bytes()).is_err()
    {
        return Err("InvalidCurseforgeFilename".into());
    }
    if !(1..=3).contains(&f.release) {
        return Err("InvalidCurseforgeReleaseType".into());
    }
    if f.games.is_empty()
        || f.games.len() > 128
        || f.games
            .iter()
            .any(|s| s.as_bytes().is_empty() || s.as_bytes().len() > 64)
    {
        return Err("InvalidCurseforgeGameVersions".into());
    }
    if f.hashes.len() > 8 {
        return Err("InvalidCurseforgeHash".into());
    }
    let mut sha1 = None;
    for hash in f.hashes {
        if hash.algo == 1 {
            if hash.value.as_bytes().len() != 40
                || !hash.value.as_bytes().iter().all(|b| b.is_ascii_hexdigit())
            {
                return Err("InvalidCurseforgeHash".into());
            }
            if sha1.is_none() {
                sha1 =
                    Some(String::from_utf8(hash.value.into_bytes()).expect("validated ASCII SHA1"))
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

#[cfg(test)]
mod typed_numeric_tests {
    use super::*;
    #[test]
    fn pure_provider_envelopes_keep_typed_numeric_tokens() {
        let h: Envelope<Object<Hash>> = decode(
            br#"{"data":{"value":"fixture","algo":1.0},"pagination":{"totalCount":4294967295.0}}"#,
        )
        .unwrap();
        assert_eq!(h.data.0.algo, 1);
        assert_eq!(h.pagination.unwrap().total_count, u32::MAX);
        assert!(
            decode::<Envelope<Object<Hash>>>(
                br#"{"data":{"value":"fixture","algo":1.0000000000000001}}"#
            )
            .is_err()
        );
        assert!(decode::<Envelope<Object<Hash>>>(br#"{"data":{"value":"fixture","algo":1},"pagination":{"totalCount":1.0000000000000001}}"#).is_err());
    }
}

#[cfg(test)]
mod opaque_metadata_tests {
    use super::*;
    #[test]
    fn ignored_project_name_and_file_date_preserve_typed_bytes() {
        for name in [r#""plain""#, "[]", "[65,255,0]", r#"["6_5",66.0]"#] {
            let json =
                format!(r#"{{"data":{{"id":1,"slug":"fixture","name":{name},"classId":6}}}}"#);
            assert!(project(json.as_bytes()).is_ok(), "{name}");
        }
        for name in ["[256]", "[65.00000000000001]", "null", "{}"] {
            let json =
                format!(r#"{{"data":{{"id":1,"slug":"fixture","name":{name},"classId":6}}}}"#);
            assert!(project(json.as_bytes()).is_err(), "{name}");
        }
        for date in [
            r#""plain""#,
            "[]",
            "[255,0]",
            r#"["6_5",66.0]"#,
            "[256]",
            "[65.00000000000001]",
            "null",
            "{}",
        ] {
            let json = format!(
                r#"{{"data":{{"id":1,"modId":1,"fileName":"fixture.jar","releaseType":1,"hashes":[{{"value":"1111111111111111111111111111111111111111","algo":1}}],"fileDate":{date},"fileLength":1,"gameVersions":["1.20.1"]}}}}"#
            );
            let expected = !["[256]", "[65.00000000000001]", "null", "{}"].contains(&date);
            assert_eq!(file(json.as_bytes()).is_ok(), expected, "{date}");
        }
    }
}

#[cfg(test)]
mod byte_field_tests {
    use super::*;
    fn response(games: &str, hashes: &str) -> Vec<u8> {
        format!(r#"{{"data":{{"id":1,"modId":1,"fileName":"fixture.jar","releaseType":1,"hashes":{hashes},"fileDate":"","fileLength":1,"gameVersions":{games}}}}}"#).into_bytes()
    }
    const SHA: &str = "1111111111111111111111111111111111111111";
    #[test]
    fn opaque_game_tags_and_ignored_hashes_are_typed_bytes() {
        let hashes = format!(r#"[{{"value":"{SHA}","algo":1}},{{"value":[255,0],"algo":2}}]"#);
        let got = file(&response("[[255,0]]", &hashes)).unwrap();
        assert_eq!(got.sha1, SHA);
        assert_eq!(got.name.as_bytes(), b"fixture.jar");
        for games in ["[]", "[[]]", &format!("[{:?}]", vec![65; 65])] {
            assert_eq!(
                file(&response(games, &hashes)).err().unwrap().to_string(),
                "InvalidCurseforgeGameVersions"
            );
        }
        let hashes = format!(r#"[{{"value":{:?},"algo":1}}]"#, vec![49; 40]);
        assert_eq!(file(&response("[[255]]", &hashes)).unwrap().sha1, SHA);
        for hash in ["[255]", "[]"] {
            let hashes = format!(r#"[{{"value":{hash},"algo":1}}]"#);
            assert_eq!(
                file(&response("[[255]]", &hashes))
                    .err()
                    .unwrap()
                    .to_string(),
                "InvalidCurseforgeHash"
            );
        }
        assert_eq!(
            file(&response("[[255]]", r#"[{"value":[255],"algo":2}]"#))
                .err()
                .unwrap()
                .to_string(),
            "MissingCurseforgeSha1"
        );
    }
    #[test]
    fn later_sha1_and_typed_byte_errors_are_not_ignored() {
        for value in ["[255]", "[]", "[256]", "[65.5]", "null", "{}"] {
            let hashes = format!(r#"[{{"value":"{SHA}","algo":1}},{{"value":{value},"algo":1}}]"#);
            assert!(file(&response(r#"["1.20.1"]"#, &hashes)).is_err());
        }
        let hashes = format!(r#"[{{"value":"{SHA}","algo":1}},{{"value":[256],"algo":2}}]"#);
        assert!(file(&response(r#"["1.20.1"]"#, &hashes)).is_err());
        let hashes = format!(r#"[{{"value":"{SHA}","algo":1}}]"#);
        for games in ["[[256]]", "[[65.5]]", "[null]", "[{}]"] {
            assert!(file(&response(games, &hashes)).is_err());
        }
        assert!(file(&response(&format!("[{:?}]", vec![255; 64]), &hashes)).is_ok());
        assert!(
            file(&response(
                &format!("[{}]", vec!["[255]"; 128].join(",")),
                &hashes
            ))
            .is_ok()
        );
        assert_eq!(
            file(&response(
                &format!("[{}]", vec!["[255]"; 129].join(",")),
                &hashes
            ))
            .err()
            .unwrap()
            .to_string(),
            "InvalidCurseforgeGameVersions"
        );
    }
}

#[cfg(test)]
mod slug_byte_tests {
    use super::*;
    #[test]
    fn project_slug_typed_bytes_validate_before_lossless_conversion() {
        for slug in [
            r#""a-Z_9.""#,
            "[97,45,90,95,57,46]",
            r#"["97",45.0,90,95,57,46]"#,
        ] {
            let raw = format!(r#"{{"data":{{"id":1,"slug":{slug},"name":"","classId":6}}}}"#);
            assert_eq!(project(raw.as_bytes()).unwrap().slug, "a-Z_9.");
        }
        for slug in [
            "[]",
            "[255]",
            "[0]",
            "[47]",
            &format!("{:?}", vec![97; 129]),
        ] {
            let raw = format!(r#"{{"data":{{"id":1,"slug":{slug},"name":"","classId":6}}}}"#);
            assert_eq!(
                project(raw.as_bytes()).err().unwrap().to_string(),
                "InvalidCurseforgeSlug"
            );
        }
        let raw = br#"{"data":{"id":0,"slug":[255],"name":"","classId":6}}"#;
        assert_eq!(
            project(raw).err().unwrap().to_string(),
            "InvalidCurseforgeId"
        );
    }
}

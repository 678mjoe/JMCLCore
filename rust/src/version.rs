//! Version service: bounded HTTP, compatible metadata cache and Vanilla planning.
//! Fabric overlays preserve parent metadata and loader-first classpath ordering.
#[path = "version_fabric.rs"]
pub mod fabric;
#[path = "version_forge.rs"]
pub mod forge;
#[path = "version_neoforge.rs"]
pub mod neoforge;
#[path = "version_zip.rs"]
pub mod zip;
use crate::{
    metadata::{self, Error, Source},
    plan::{self, Target},
    protocol::RpcError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, value::RawValue};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct Response {
    pub source: Source,
    pub result: Value,
}
pub trait Transport {
    fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, Error>;
    fn head_size(&self, _url: &str) -> Result<u64, Error> {
        Err(Error::Network)
    }
}
pub struct HttpTransport(reqwest::blocking::Client);
impl HttpTransport {
    pub fn new() -> Result<Self, Error> {
        reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::limited(3))
            .timeout(Duration::from_secs(30))
            .user_agent("jmcl-core/0.1.0-dev")
            .build()
            .map(Self)
            .map_err(|_| Error::Network)
    }
}
impl Transport for HttpTransport {
    fn head_size(&self, url: &str) -> Result<u64, Error> {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .user_agent("jmcl-core/0.1.0-dev")
            .build()
            .map_err(|_| Error::Network)?;
        let response = client.head(url).send().map_err(|e| {
            if e.is_timeout() {
                Error::MetadataFetchTimeout
            } else {
                Error::Network
            }
        })?;
        if !response.status().is_success() {
            return Err(Error::UnsuccessfulHttpStatus);
        }
        response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or(Error::Plan("MissingContentLength"))
    }

    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, Error> {
        let response = self.0.get(url).send().map_err(|e| {
            if e.is_timeout() {
                Error::MetadataFetchTimeout
            } else {
                Error::Network
            }
        })?;
        finalize_metadata_body(
            response.status().is_success(),
            response.content_length(),
            response,
            max,
        )
    }
}
// Shared by the actual GET adapter and injected-reader contract tests.
fn finalize_metadata_body(
    successful: bool,
    _advertised_length: Option<u64>,
    response: impl Read,
    max: usize,
) -> Result<Vec<u8>, Error> {
    // The pinned fetchBodyWithHeader consumes a bounded body before status
    // classification. Content-Length is not a substitute for bytes actually
    // produced by the response reader (which owns framing validation).
    let mut bytes = Vec::new();
    response
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                Error::MetadataFetchTimeout
            } else {
                Error::Network
            }
        })?;
    if bytes.len() > max {
        return Err(Error::ResponseBodyTooLarge);
    }
    if !successful {
        return Err(Error::UnsuccessfulHttpStatus);
    }
    Ok(bytes)
}
#[cfg(test)]
#[path = "version_http_tests.rs"]
mod http_tests;
#[derive(Clone, Copy)]
pub enum Policy {
    Immutable,
    Ttl(u64),
}
#[derive(Serialize)]
#[serde(untagged)]
enum Payload {
    Text(String),
    Bytes(Vec<u8>),
}
// Cache envelopes are typed Zig JSON, not dynamic RPC numbers. Retain each
// lexeme until the shared binary128/typed-integer coercion has run.
impl<'de> Deserialize<'de> for Payload {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = Box::<RawValue>::deserialize(d)?;
        if raw.get().starts_with('"') {
            return serde_json::from_str(raw.get())
                .map(Self::Text)
                .map_err(D::Error::custom);
        }
        let values: Vec<Box<RawValue>> =
            serde_json::from_str(raw.get()).map_err(D::Error::custom)?;
        values
            .iter()
            .map(|v| {
                crate::json_number::typed_raw(v.get(), 8)
                    .map(|n| n as u8)
                    .map_err(|e| D::Error::custom(e.name()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self::Bytes)
    }
}
fn cache_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    use serde::de::Error;
    let raw = Box::<RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), 32)
        .map(|n| n as u32)
        .map_err(|e| D::Error::custom(e.name()))
}
fn cache_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    use serde::de::Error;
    let raw = Box::<RawValue>::deserialize(d)?;
    let text = if raw.get().starts_with('"') {
        serde_json::from_str::<String>(raw.get()).map_err(D::Error::custom)?
    } else {
        raw.get().to_owned()
    };
    let negative = text.starts_with('-');
    let magnitude = text.strip_prefix(['+', '-']).unwrap_or(&text);
    if magnitude.starts_with(['+', '-']) {
        return Err(D::Error::custom("InvalidCharacter"));
    }
    let n =
        crate::json_number::unsigned_text(magnitude, 64).map_err(|e| D::Error::custom(e.name()))?;
    if negative && n == 1u64 << 63 {
        return Ok(i64::MIN);
    }
    let n = i64::try_from(n).map_err(|_| D::Error::custom("Overflow"))?;
    Ok(if negative { -n } else { n })
}
fn cache_url<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::de::Error;
    // Requested URLs are UTF-8. A non-UTF-8 stored URL can never equal one;
    // rejecting it has the same cache-miss/eviction effect as the source check.
    String::from_utf8(Payload::deserialize(d)?.bytes()).map_err(D::Error::custom)
}
impl Payload {
    fn bytes(self) -> Vec<u8> {
        match self {
            Self::Text(s) => s.into_bytes(),
            Self::Bytes(b) => b,
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Bytes(b) => b.len(),
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    #[serde(deserialize_with = "cache_u32")]
    v: u32,
    #[serde(deserialize_with = "cache_url")]
    url: String,
    #[serde(deserialize_with = "cache_i64")]
    fetched_at: i64,
    payload: Payload,
}
pub struct Service<T: Transport> {
    pub transport: T,
    pub cache_root: PathBuf,
    pub now: i64,
}
impl<T: Transport> Service<T> {
    pub fn new(transport: T, cache_root: impl Into<PathBuf>) -> Self {
        Self {
            transport,
            cache_root: cache_root.into(),
            now: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .min(i64::MAX as u64) as i64,
        }
    }
    fn cache_path(&self, url: &str) -> PathBuf {
        self.cache_root
            .join(format!("{:x}.json", Sha1::digest(url.as_bytes())))
    }
    fn evict(&self, url: &str) {
        let _ = fs::remove_file(self.cache_path(url));
    }
    fn lookup(&self, url: &str, max: usize, policy: Policy) -> Option<(Vec<u8>, bool)> {
        let path = self.cache_path(url);
        let invalid = || {
            self.evict(url);
            None
        };
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => return invalid(),
        };
        let limit = max.checked_add(64 * 1024)?;
        let mut bytes = Vec::new();
        if file.take(limit as u64 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > limit {
            return invalid();
        }
        // Ignored fields still must contain well-formed JSON strings, as in the
        // original typed parser. Every malformed envelope has the same public
        // miss/eviction outcome, so this does not change error precedence.
        if std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| crate::auth::json::Json::parse(text).ok())
            .is_none()
        {
            return invalid();
        }
        let env: Envelope = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return invalid(),
        };
        if env.v != 1 || env.url != url || env.payload.len() > max {
            return invalid();
        }
        let age = self.now.saturating_sub(env.fetched_at).max(0) as u64;
        let fresh = match policy {
            Policy::Immutable => true,
            Policy::Ttl(ttl) => age <= ttl,
        };
        if !fresh && age > 86400 {
            return invalid();
        }
        Some((env.payload.bytes(), fresh))
    }
    fn store(&self, url: &str, bytes: &[u8]) {
        let payload = match std::str::from_utf8(bytes) {
            Ok(s) => Payload::Text(s.into()),
            Err(_) => Payload::Bytes(bytes.to_vec()),
        };
        if fs::create_dir_all(&self.cache_root).is_err() {
            return;
        }
        let env = Envelope {
            v: 1,
            url: url.into(),
            fetched_at: self.now,
            payload,
        };
        let Ok(bytes) = serde_json::to_vec(&env) else {
            return;
        };
        static SEQ: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let path = self.cache_root.join(format!(
                ".jmcl-cache-{}-{}-{}",
                std::process::id(),
                self.now,
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let mut f = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return,
            };
            let result = f.write_all(&bytes).and_then(|()| f.sync_all());
            drop(f);
            if result.is_ok() {
                let _ = fs::rename(&path, self.cache_path(url));
            }
            let _ = fs::remove_file(path);
            return;
        }
    }
    pub fn cached(&self, url: &str, max: usize, policy: Policy) -> Result<(Vec<u8>, bool), Error> {
        let cached = self.lookup(url, max, policy);
        if let Some((bytes, true)) = &cached {
            return Ok((bytes.clone(), true));
        }
        match self.transport.get(url, max) {
            Ok(bytes) => {
                if bytes.len() > max {
                    return Err(Error::ResponseBodyTooLarge);
                }
                self.store(url, &bytes);
                Ok((bytes, false))
            }
            Err(e) => {
                if let Some((bytes, _)) = cached {
                    Ok((bytes, true))
                } else {
                    Err(e)
                }
            }
        }
    }
    pub fn manifest(&self, source: Source) -> Result<Value, Error> {
        metadata::manifest(
            &self
                .cached(source.manifest_url(), 2 * 1024 * 1024, Policy::Ttl(600))?
                .0,
        )
    }
    /// Listing-only byte view; acquisition keeps the original String manifest.
    pub fn listing_manifest(&self, source: Source) -> Result<metadata::listing::Listing, Error> {
        metadata::listing::Listing::parse(
            &self
                .cached(source.manifest_url(), 2 * 1024 * 1024, Policy::Ttl(600))?
                .0,
        )
    }
    pub fn get(&self, id: &str, source: Source) -> Result<Value, Error> {
        let manifest = self.manifest(source)?;
        let selected = manifest["versions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .ok_or(Error::VersionNotFound)?;
        let official = selected["url"].as_str().unwrap();
        let url = source.rewrite(official).unwrap_or_else(|| official.into());
        let (mut bytes, cached) = self.cached(&url, 4 * 1024 * 1024, Policy::Immutable)?;
        if let Some(hash) = selected["sha1"].as_str()
            && !metadata::verify_sha1(&bytes, hash)
        {
            if !cached {
                return Err(Error::MetadataHashMismatch);
            }
            self.evict(&url);
            bytes = self.cached(&url, 4 * 1024 * 1024, Policy::Immutable)?.0;
            if !metadata::verify_sha1(&bytes, hash) {
                return Err(Error::MetadataHashMismatch);
            }
        }
        let version = metadata::metadata(&bytes)?;
        if version["id"] != id {
            return Err(Error::MetadataIdMismatch);
        }
        Ok(version)
    }
    pub fn get_preferred(&self, id: &str, source: Source) -> Result<Value, Error> {
        self.get(id, source).or_else(|e| {
            if source == Source::Bmclapi {
                self.get(id, Source::Official)
            } else {
                Err(e)
            }
        })
    }
    pub fn resolve(&self, id: &str, loader: Option<&str>, source: Source) -> Result<Value, Error> {
        let parent = self.get(id, source)?;
        let Some(loader) = loader else {
            return Ok(parent);
        };
        fabric::profile_id(id)?;
        fabric::profile_id(loader)?;
        let url =
            format!("https://meta.fabricmc.net/v2/versions/loader/{id}/{loader}/profile/json");
        let p = fabric::parse(&self.cached(&url, 4 * 1024 * 1024, Policy::Ttl(600))?.0)?;
        if p["inheritsFrom"] != id {
            return Err(Error::Plan("FabricParentMismatch"));
        }
        if p["id"] != format!("fabric-loader-{loader}-{id}") {
            return Err(Error::Plan("FabricProfileIdMismatch"));
        }
        let mut integrities = Vec::new();
        for l in p["libraries"].as_array().unwrap() {
            let url = format!(
                "{}{}",
                l["url"].as_str().unwrap(),
                fabric::artifact_path(l["name"].as_str().unwrap())?
            );
            let hash = if let Some(hash) = l["sha1"].as_str() {
                hash.to_owned()
            } else {
                let bytes = self
                    .cached(&format!("{url}.sha1"), 4096, Policy::Immutable)?
                    .0;
                let token = bytes
                    .split(|c| b" \t\r\n".contains(c))
                    .next()
                    .unwrap_or_default();
                let hash =
                    std::str::from_utf8(token).map_err(|_| Error::Plan("InvalidLibrarySha1"))?;
                fabric::sha1(hash)?;
                hash.into()
            };
            let n = if let Some(n) = l["size"].as_u64() {
                n
            } else {
                self.content_length(&url)?
            };
            if n == 0 || n > 256 * 1024 * 1024 {
                return Err(Error::Plan("InvalidFabricLibrarySize"));
            }
            integrities.push((hash, n));
        }
        fabric::merge(&parent, &p, &integrities)
    }
    pub fn resolve_preferred(
        &self,
        id: &str,
        loader: Option<&str>,
        source: Source,
    ) -> Result<Value, Error> {
        self.resolve(id, loader, source).or_else(|e| {
            if source == Source::Bmclapi {
                self.resolve(id, loader, Source::Official)
            } else {
                Err(e)
            }
        })
    }
    fn content_length(&self, url: &str) -> Result<u64, Error> {
        let key = format!("head:{url}");
        if let Some((bytes, _)) = self.lookup(&key, 32, Policy::Immutable) {
            if let Ok(s) = std::str::from_utf8(&bytes)
                && let Ok(n) = s.parse()
            {
                return Ok(n);
            }
            self.evict(&key);
        }
        let n = self.transport.head_size(url)?;
        self.store(&key, n.to_string().as_bytes());
        Ok(n)
    }
    pub fn resolve_neoforge(
        &self,
        id: &str,
        loader: &str,
        source: Source,
    ) -> Result<(Value, Value, String, u64), Error> {
        neoforge::version_id(loader)?;
        let parent = self.get(id, source)?;
        let url = format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader}/neoforge-{loader}-installer.jar"
        );
        let bytes = self.cached(&url, 64 * 1024 * 1024, Policy::Immutable)?.0;
        let vp = zip::read_entry(&bytes, "version.json", 4 * 1024 * 1024)?
            .ok_or(Error::Plan("NeoForgeVersionProfileMissing"))?;
        let ip = zip::read_entry(&bytes, "install_profile.json", 16 * 1024 * 1024)?
            .ok_or(Error::Plan("NeoForgeInstallProfileMissing"))?;
        let profile = neoforge::version(&vp)?;
        let install = neoforge::install(&ip)?;
        if install["minecraft"] != id || profile["inheritsFrom"] != id {
            return Err(Error::Plan("NeoForgeParentMismatch"));
        }
        let expected = format!("neoforge-{loader}");
        if install["version"] != expected || profile["id"] != expected {
            return Err(Error::Plan("NeoForgeProfileIdMismatch"));
        }
        let merged = neoforge::merge(&parent, &profile, &neoforge::patched_path(&install, false)?)?;
        Ok((
            merged,
            install,
            format!("{:x}", Sha1::digest(&bytes)),
            bytes.len() as u64,
        ))
    }
    pub fn resolve_neoforge_preferred(
        &self,
        id: &str,
        loader: &str,
        source: Source,
    ) -> Result<Value, Error> {
        self.resolve_neoforge(id, loader, source)
            .or_else(|e| {
                if source == Source::Bmclapi {
                    self.resolve_neoforge(id, loader, Source::Official)
                } else {
                    Err(e)
                }
            })
            .map(|v| v.0)
    }
    pub fn plan_neoforge(
        &self,
        id: &str,
        loader: &str,
        source: Source,
        target: Target,
    ) -> Result<plan::Plan, Error> {
        self.plan_neoforge_from(id, loader, source, source, target.clone())
            .or_else(|e| {
                if source == Source::Bmclapi {
                    self.plan_neoforge_from(id, loader, Source::Official, source, target)
                } else {
                    Err(e)
                }
            })
    }
    fn plan_neoforge_from(
        &self,
        id: &str,
        loader: &str,
        metadata_source: Source,
        download_source: Source,
        target: Target,
    ) -> Result<plan::Plan, Error> {
        let (metadata, install, hash, size) = self.resolve_neoforge(id, loader, metadata_source)?;
        let assets = if metadata["assetIndex"].is_null() {
            None
        } else {
            Some(self.assets(&metadata["assetIndex"], metadata_source)?)
        };
        let mut extras = vec![plan::File {
            path: format!(".jmcl/neoforge/{loader}/installer.jar"),
            url: format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader}/neoforge-{loader}-installer.jar"
            ),
            fallback_url: None,
            sha1: hash,
            size,
            usage: plan::Usage {
                library: true,
                ..Default::default()
            },
        }];
        for l in install["libraries"].as_array().unwrap() {
            let a = &l["downloads"]["artifact"];
            let path = format!(
                "libraries/{}",
                neoforge::artifact_path(l["name"].as_str().unwrap())?
            );
            if let Some(existing) = extras.iter().find(|f| f.path == path) {
                if !existing
                    .sha1
                    .eq_ignore_ascii_case(a["sha1"].as_str().unwrap())
                    || existing.size != a["size"].as_u64().unwrap()
                {
                    return Err(Error::Plan("ConflictingDestination"));
                }
                continue;
            }
            extras.push(plan::File {
                path,
                url: a["url"].as_str().unwrap().into(),
                fallback_url: None,
                sha1: a["sha1"].as_str().unwrap().into(),
                size: a["size"].as_u64().unwrap(),
                usage: plan::Usage {
                    library: true,
                    ..Default::default()
                },
            });
        }
        let mut plan =
            plan::build_with_extras(&metadata, assets.as_ref(), download_source, target, &extras)?;
        plan.loader = "neoforge".into();
        Ok(plan)
    }
    pub fn assets(&self, descriptor: &Value, source: Source) -> Result<Value, Error> {
        let original = descriptor["url"].as_str().ok_or(Error::InvalidJson)?;
        let url = source.rewrite(original).unwrap_or_else(|| original.into());
        let bytes = self.transport.get(&url, 16 * 1024 * 1024)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(Error::ResponseBodyTooLarge);
        }
        if !metadata::verify_sha1(
            &bytes,
            descriptor["sha1"].as_str().ok_or(Error::InvalidJson)?,
        ) {
            return Err(Error::AssetIndexHashMismatch);
        }
        metadata::asset_index(&bytes)
    }
    pub fn plan(&self, id: &str, source: Source, target: Target) -> Result<plan::Plan, Error> {
        self.plan_loader(id, None, source, target)
    }
    pub fn plan_loader(
        &self,
        id: &str,
        loader: Option<&str>,
        source: Source,
        target: Target,
    ) -> Result<plan::Plan, Error> {
        self.plan_from(id, loader, source, source, target.clone())
            .or_else(|e| {
                if source == Source::Bmclapi {
                    self.plan_from(id, loader, Source::Official, source, target)
                } else {
                    Err(e)
                }
            })
    }
    fn plan_from(
        &self,
        id: &str,
        loader: Option<&str>,
        metadata_source: Source,
        download_source: Source,
        target: Target,
    ) -> Result<plan::Plan, Error> {
        let version = self.resolve(id, loader, metadata_source)?;
        let assets = if version["assetIndex"].is_null() {
            None
        } else {
            Some(self.assets(&version["assetIndex"], metadata_source)?)
        };
        let mut plan = plan::build(&version, assets.as_ref(), download_source, target)?;
        if loader.is_some() {
            plan.loader = "fabric".into();
        }
        Ok(plan)
    }
}
fn invalid(message: &str) -> RpcError {
    RpcError::new("INVALID_PARAMS", message)
}
fn parse_params(params: Option<&RawValue>) -> Result<Option<Value>, Error> {
    params
        .map(|v| metadata::parse_json(v.get().as_bytes()))
        .transpose()
}
// Canonicalize declared enums and UTF-8 byte arrays for these request schemas.
// Borrow ordinary strings; clone only for accepted alternative representations.
// The caller's Value and unrelated fields remain unchanged.
fn canonical_params_view<'a>(
    method: &str,
    value: Option<&'a Value>,
) -> Result<Option<std::borrow::Cow<'a, Value>>, RpcError> {
    use std::borrow::Cow;
    if !matches!(
        method,
        "version.list" | "version.get" | "version.resolve" | "install.plan"
    ) {
        return Ok(value.map(Cow::Borrowed));
    }
    fn tag(value: &Value, tags: &[&str]) -> Option<usize> {
        let ordinal = match value {
            Value::String(text) => {
                if let Some(index) = tags.iter().position(|tag| *tag == text) {
                    return Some(index);
                }
                // Zig's dynamic enum parser accepts integer-shaped strings,
                // but not float spellings (including the special string -0).
                if text == "-0" || text.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
                    return None;
                }
                crate::json_number::unsigned_text(text, 64).ok()?
            }
            // Do not coerce floating Values, even an integral 0.0 or -0.
            Value::Number(number) => number.as_u64()?,
            _ => return None,
        };
        let index = usize::try_from(ordinal).ok()?;
        (index < tags.len()).then_some(index)
    }
    let Some(value) = value else { return Ok(None) };
    let fields = value.as_object().ok_or_else(|| param_error(method))?;
    let mut view = Cow::Borrowed(value);
    for (key, tags, nullable, relevant) in [
        ("source", &["official", "bmclapi"][..], false, true),
        (
            "type",
            &["release", "snapshot", "old_beta", "old_alpha"][..],
            true,
            method == "version.list",
        ),
        (
            "os",
            &["windows", "linux", "osx"][..],
            true,
            method == "install.plan",
        ),
        (
            "arch",
            &["x86", "x86_64", "arm", "arm64"][..],
            true,
            method == "install.plan",
        ),
    ] {
        if !relevant {
            continue;
        }
        let Some(field) = fields.get(key) else {
            continue;
        };
        if nullable && field.is_null() {
            continue;
        }
        let name = tags[tag(field, tags).ok_or_else(|| param_error(method))?];
        if field.as_str() != Some(name) {
            view.to_mut()
                .as_object_mut()
                .expect("object checked")
                .insert(key.into(), Value::String(name.into()));
        }
    }
    for (key, nullable, relevant) in [
        ("id", false, method != "version.list"),
        (
            "fabric_loader",
            true,
            matches!(method, "version.resolve" | "install.plan"),
        ),
        (
            "neoforge_version",
            true,
            matches!(method, "version.resolve" | "install.plan"),
        ),
        (
            "forge_version",
            true,
            matches!(method, "version.resolve" | "install.plan"),
        ),
    ] {
        if !relevant {
            continue;
        }
        let Some(field) = fields.get(key) else {
            continue;
        };
        if field.is_string() || (nullable && field.is_null()) {
            continue;
        }
        let elements = field.as_array().ok_or_else(|| param_error(method))?;
        let bytes = elements
            .iter()
            .map(|element| {
                // Value numbers already underwent dynamic binary64 parsing; strings
                // retain their exact contents for the typed quoted-number branch.
                crate::json_number::dynamic_raw(&element.to_string(), 8)
                    .map(|byte| byte as u8)
                    .map_err(|_| param_error(method))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Opaque byte IDs/loaders still need their source-ordered operational
        // consumers. Never silently replace bytes or mislabel early rejection.
        let text = String::from_utf8(bytes).map_err(|_| param_error(method))?;
        view.to_mut()
            .as_object_mut()
            .expect("object checked")
            .insert(key.into(), Value::String(text));
    }
    Ok(Some(view))
}
fn source(p: &Map<String, Value>) -> Result<Source, Error> {
    p.get("source")
        .map(|s| s.as_str().and_then(Source::parse).ok_or(Error::InvalidJson))
        .unwrap_or(Ok(Source::Official))
}
fn loader_count(p: &Map<String, Value>) -> Result<usize, Error> {
    let mut count = 0;
    for k in ["fabric_loader", "neoforge_version", "forge_version"] {
        if let Some(v) = p.get(k)
            && !v.is_null()
        {
            if !v.is_string() {
                return Err(Error::InvalidJson);
            }
            count += 1
        }
    }
    Ok(count)
}
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Response, RpcError>> {
    if !matches!(
        method,
        "version.list" | "version.get" | "version.resolve" | "install.plan"
    ) {
        return None;
    }
    // Parse before client construction so invalid requests never trigger network setup.
    let parsed = match parse_params(params) {
        Ok(x) => x,
        Err(_) => return Some(Err(param_error(method))),
    };
    let canonical = match canonical_params_view(method, parsed.as_ref()) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    let parsed = canonical.as_deref();
    if let Err(e) = validate_params(method, parsed) {
        return Some(Err(e));
    }
    let transport = match HttpTransport::new() {
        Ok(t) => t,
        Err(e) => return Some(Err(map_error(method, e))),
    };
    Some(handle_with(
        &Service::new(transport, Path::new(".jmclcore/metadata-cache")),
        method,
        parsed,
    ))
}
fn param_error(method: &str) -> RpcError {
    invalid(match method {
        "version.list" => "version.list params must contain a valid type and limit",
        "version.get" => "version.get requires a string id",
        "version.resolve" => "version.resolve parameters are invalid",
        _ => "install.plan parameters are invalid",
    })
}
fn validate_params(method: &str, value: Option<&Value>) -> Result<(), RpcError> {
    if method == "version.list" {
        let empty = Map::new();
        let p = if let Some(v) = value {
            v.as_object().ok_or_else(|| param_error(method))?
        } else {
            &empty
        };
        source(p).map_err(|_| param_error(method))?;
        if let Some(v) = p.get("type")
            && !v.is_null()
            && !matches!(
                v.as_str(),
                Some("release" | "snapshot" | "old_beta" | "old_alpha")
            )
        {
            return Err(param_error(method));
        }
        let limit = p
            .get("limit")
            .map(|v| metadata::unsigned(v, u64::MAX))
            .transpose()
            .map_err(|_| param_error(method))?
            .unwrap_or(20);
        if !(1..=1000).contains(&limit) {
            return Err(invalid("limit must be between 1 and 1000"));
        }
        return Ok(());
    }
    let v = value.ok_or_else(|| {
        invalid(match method {
            "version.get" => "version.get requires an id",
            "version.resolve" => "version.resolve requires an id",
            _ => "install.plan requires a version id",
        })
    })?;
    let p = v.as_object().ok_or_else(|| param_error(method))?;
    let id = p
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| param_error(method))?;
    source(p).map_err(|_| param_error(method))?;
    let count = if method != "version.get" {
        loader_count(p).map_err(|_| param_error(method))?
    } else {
        0
    };
    if method == "install.plan" {
        for (k, allowed) in [
            ("os", &["windows", "linux", "osx"][..]),
            ("arch", &["x86", "x86_64", "arm", "arm64"][..]),
        ] {
            if let Some(v) = p.get(k)
                && !v.is_null()
                && !v.as_str().is_some_and(|s| allowed.contains(&s))
            {
                return Err(param_error(method));
            }
        }
    }
    if id.is_empty() {
        return Err(invalid("version id must not be empty"));
    }
    if method == "version.resolve" && count > 1 {
        return Err(invalid("only one loader field may be set"));
    }
    Ok(())
}
pub fn handle_with<T: Transport>(
    service: &Service<T>,
    method: &str,
    value: Option<&Value>,
) -> Result<Response, RpcError> {
    let canonical = canonical_params_view(method, value)?;
    let value = canonical.as_deref();
    validate_params(method, value)?;
    let empty = Map::new();
    let p = value.and_then(Value::as_object).unwrap_or(&empty);
    let source = source(p).map_err(|_| param_error(method))?;
    let id = p.get("id").and_then(Value::as_str).unwrap_or("");
    let fabric = p.get("fabric_loader").and_then(Value::as_str);
    let neo = p.get("neoforge_version").and_then(Value::as_str);
    let forge = p.get("forge_version").and_then(Value::as_str);
    let result = match method {
        "version.list" => {
            let kind = p.get("type").and_then(Value::as_str);
            let limit = p
                .get("limit")
                .map(|x| metadata::unsigned(x, u64::MAX).unwrap())
                .unwrap_or(20) as usize;
            service
                .listing_manifest(source)
                .map(|m| m.selection(kind, limit))
        }
        "version.get" => service.get(id, source),
        "version.resolve" => {
            let resolved = if let Some(loader) = forge {
                service
                    .resolve_forge_preferred(id, loader, source)
                    .map(|r| (r.metadata, r.kind))
            } else if let Some(loader) = neo {
                service
                    .resolve_neoforge_preferred(id, loader, source)
                    .map(|m| (m, "neoforge"))
            } else {
                service.resolve_preferred(id, fabric, source).map(|m| {
                    (
                        m,
                        if fabric.is_some() {
                            "fabric"
                        } else {
                            "vanilla"
                        },
                    )
                })
            };
            resolved.map(|(m, kind)| {
                let mut out = Map::new();
                out.insert("kind".into(), kind.into());
                out.insert("id".into(), m["id"].clone());
                out.insert(
                    "java_major_version".into(),
                    m["javaVersion"]["majorVersion"].clone(),
                );
                Value::Object(out)
            })
        }
        "install.plan" => {
            if loader_count(p).unwrap() > 1 {
                return Err(map_error(method, Error::Plan("LoaderConflict")));
            }
            let mut target = Target::host().map_err(|_| {
                RpcError::new("UNSUPPORTED_PLATFORM", "The host platform is not supported")
            })?;
            if let Some(v) = p.get("os").and_then(Value::as_str) {
                target.os = v.into()
            }
            if let Some(v) = p.get("arch").and_then(Value::as_str) {
                target.arch = v.into()
            }
            let plan = if let Some(loader) = forge {
                service.plan_forge(id, loader, source, target)
            } else if let Some(loader) = neo {
                service.plan_neoforge(id, loader, source, target)
            } else {
                service.plan_loader(id, fabric, source, target)
            };
            plan.map(|p| serde_json::to_value(p).unwrap())
        }
        _ => return Err(RpcError::new("METHOD_NOT_FOUND", "Unknown RPC method")),
    }
    .map_err(|e| map_error(method, e))?;
    Ok(Response { source, result })
}
pub fn map_error(method: &str, e: Error) -> RpcError {
    let (code, message) = match method {
        "version.list" => match e {
            Error::MetadataFetchTimeout => (
                "METADATA_TIMEOUT",
                "Timed out fetching the version manifest",
            ),
            Error::ResponseBodyTooLarge => (
                "RESPONSE_TOO_LARGE",
                "Version manifest exceeds the size limit",
            ),
            Error::UnsuccessfulHttpStatus => (
                "HTTP_STATUS_ERROR",
                "The selected download source returned an unsuccessful HTTP status",
            ),
            Error::InvalidJson
            | Error::Json(
                "SyntaxError" | "UnexpectedToken" | "UnknownField" | "InvalidEnumTag"
                | "MissingField",
            ) => (
                "INVALID_MANIFEST",
                "Mojang returned an invalid version manifest",
            ),
            _ => (
                "NETWORK_ERROR",
                "Failed to fetch the version manifest from the selected source",
            ),
        },
        "version.get" => match e {
            Error::MetadataFetchTimeout => {
                ("METADATA_TIMEOUT", "Timed out fetching version metadata")
            }
            Error::VersionNotFound => (
                "VERSION_NOT_FOUND",
                "The requested version is not in the Mojang manifest",
            ),
            Error::MetadataHashMismatch => (
                "INTEGRITY_ERROR",
                "Version metadata failed SHA-1 verification",
            ),
            Error::MetadataIdMismatch => (
                "INVALID_METADATA",
                "Version metadata contains an unexpected id",
            ),
            Error::ResponseBodyTooLarge => (
                "RESPONSE_TOO_LARGE",
                "Mojang metadata exceeds the size limit",
            ),
            Error::UnsuccessfulHttpStatus => (
                "HTTP_STATUS_ERROR",
                "The selected download source returned an unsuccessful HTTP status",
            ),
            Error::InvalidJson
            | Error::Json(
                "SyntaxError" | "UnexpectedToken" | "UnknownField" | "InvalidEnumTag"
                | "MissingField",
            ) => (
                "INVALID_METADATA",
                "Mojang returned invalid version metadata",
            ),
            _ => (
                "NETWORK_ERROR",
                "Failed to fetch version metadata from the selected source",
            ),
        },
        "version.resolve" => match e {
            Error::MetadataFetchTimeout => (
                "METADATA_TIMEOUT",
                "Timed out while resolving the Minecraft version and loader",
            ),
            Error::VersionNotFound => (
                "VERSION_NOT_FOUND",
                "The requested version is not in the version manifest",
            ),
            _ => (
                "VERSION_RESOLUTION_FAILED",
                "Failed to resolve the Minecraft version and loader",
            ),
        },
        _ => match e {
            Error::VersionNotFound => (
                "VERSION_NOT_FOUND",
                "The requested version is not in the selected manifest",
            ),
            Error::MetadataHashMismatch | Error::AssetIndexHashMismatch => (
                "INTEGRITY_ERROR",
                "Downloaded metadata failed SHA-1 verification",
            ),
            Error::ResponseBodyTooLarge => (
                "RESPONSE_TOO_LARGE",
                "Version metadata exceeds the size limit",
            ),
            Error::UnsuccessfulHttpStatus => (
                "HTTP_STATUS_ERROR",
                "The selected download source returned an unsuccessful HTTP status",
            ),
            Error::Plan("UnsafePath" | "UnsafeArchivePath" | "InsecureDownloadUrl") => (
                "UNSAFE_METADATA",
                "Metadata contains an unsafe download destination",
            ),
            Error::Plan(
                "InvalidSha1"
                | "InvalidAssetHash"
                | "ConflictingDestination"
                | "ConflictingProjection"
                | "MissingLibraryDownloads"
                | "MissingLibraryArtifact"
                | "MissingNativeClassifiers"
                | "MissingNativeClassifier",
            ) => (
                "INVALID_METADATA",
                "Metadata cannot produce a consistent install plan",
            ),
            _ => ("INSTALL_PLAN_ERROR", "Failed to create the install plan"),
        },
    };
    RpcError::new(code, message)
}

/// Only claims the version list/show and Vanilla install plan CLI subcommands.
/// Return None for unrelated commands so other rewrite slices can own them.
pub fn cli(
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<std::io::Result<u8>> {
    let method = cli_method(args)?;
    let (params, json) = match cli_params(method, &args[2..]) {
        Ok(x) => x,
        Err(()) => return Some(writeln!(err, "{}", cli_usage(method)).map(|_| 2)),
    };
    let transport = match HttpTransport::new() {
        Ok(x) => x,
        Err(e) => return Some(cli_failure(method, &params, e, err)),
    };
    Some(cli_execute(
        &Service::new(transport, ".jmclcore/metadata-cache"),
        method,
        &params,
        json,
        out,
        err,
    ))
}
fn cli_method(args: &[String]) -> Option<&'static str> {
    match (
        args.first().map(String::as_str),
        args.get(1).map(String::as_str),
    ) {
        (Some("version"), Some("list")) => Some("version.list"),
        (Some("version"), Some("show")) => Some("version.get"),
        (Some("install"), Some("plan")) => Some("install.plan"),
        _ => None,
    }
}
fn cli_usage(method: &str) -> &'static str {
    match method {
        "version.list" => {
            "usage: jmcl-core version list [--type TYPE] [--limit N] [--source SOURCE] [--json]"
        }
        "version.get" => "usage: jmcl-core version show ID [--source SOURCE] [--json]",
        _ => {
            "usage: jmcl-core install plan ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--source SOURCE] [--os OS] [--arch ARCH] [--json]"
        }
    }
}
fn cli_params(method: &str, args: &[String]) -> Result<(Value, bool), ()> {
    let mut p = Map::new();
    let mut i = 0;
    let mut json = false;
    if method != "version.list" {
        let id = args.first().filter(|s| !s.is_empty()).ok_or(())?;
        p.insert("id".into(), id.clone().into());
        i = 1;
    }
    while i < args.len() {
        let key = &args[i];
        if key == "--json" {
            json = true;
            i += 1;
            continue;
        }
        let value = args.get(i + 1).ok_or(())?;
        let key = match key.as_str() {
            "--source" => {
                Source::parse(value).ok_or(())?;
                "source"
            }
            "--type" if method == "version.list" => {
                if !matches!(
                    value.as_str(),
                    "release" | "snapshot" | "old_beta" | "old_alpha"
                ) {
                    return Err(());
                }
                "type"
            }
            "--limit" if method == "version.list" => {
                let n = value.replace('_', "").parse::<u64>().map_err(|_| ())?;
                if !(1..=1000).contains(&n) {
                    return Err(());
                }
                p.insert("limit".into(), n.into());
                i += 2;
                continue;
            }
            "--os" if method == "install.plan" => {
                if !matches!(value.as_str(), "windows" | "linux" | "osx") {
                    return Err(());
                }
                "os"
            }
            "--arch" if method == "install.plan" => {
                let arch = match value.as_str() {
                    "amd64" => "x86_64",
                    "aarch64" => "arm64",
                    "x86" | "x86_64" | "arm" | "arm64" => value.as_str(),
                    _ => return Err(()),
                };
                p.insert("arch".into(), arch.into());
                i += 2;
                continue;
            }
            "--fabric-loader" if method == "install.plan" && !value.is_empty() => "fabric_loader",
            "--neoforge-version" if method == "install.plan" && !value.is_empty() => {
                "neoforge_version"
            }
            "--forge-version" if method == "install.plan" && !value.is_empty() => "forge_version",
            _ => return Err(()),
        };
        p.insert(key.into(), value.clone().into());
        i += 2;
    }
    Ok((Value::Object(p), json))
}
fn cli_failure(method: &str, p: &Value, e: Error, err: &mut impl Write) -> std::io::Result<u8> {
    let source = p
        .get("source")
        .and_then(Value::as_str)
        .and_then(Source::parse)
        .unwrap_or_default();
    match method {
        "version.list" => writeln!(
            err,
            "failed to fetch version manifest from {}: {}",
            source.display(),
            e.name()
        )?,
        "version.get" if e == Error::VersionNotFound => writeln!(
            err,
            "Minecraft version not found: {}",
            p["id"].as_str().unwrap()
        )?,
        "version.get" => writeln!(
            err,
            "failed to fetch Minecraft version metadata from {}: {}",
            source.display(),
            e.name()
        )?,
        _ => writeln!(
            err,
            "failed to create install plan from {}: {}",
            source.display(),
            e.name()
        )?,
    }
    Ok(1)
}
fn cli_execute<T: Transport>(
    svc: &Service<T>,
    method: &str,
    p: &Value,
    json: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<u8> {
    let source = p
        .get("source")
        .and_then(Value::as_str)
        .and_then(Source::parse)
        .unwrap_or_default();
    if method == "version.list" {
        let listing = match svc.listing_manifest(source) {
            Ok(listing) => listing,
            Err(e) => return cli_failure(method, p, e, err),
        };
        listing.write(
            out,
            source,
            p.get("type").and_then(Value::as_str),
            p.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize,
            json,
        )?;
        return Ok(0);
    }
    let id = p.get("id").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "version.get" => svc.get(id, source),
        _ => {
            let count = loader_count(p.as_object().unwrap()).unwrap();
            if count > 1 {
                Err(Error::Plan("LoaderConflict"))
            } else {
                let mut target = match Target::host() {
                    Ok(t) => t,
                    Err(e) => {
                        writeln!(err, "unsupported host platform: {}", e.name())?;
                        return Ok(1);
                    }
                };
                if let Some(v) = p.get("os").and_then(Value::as_str) {
                    target.os = v.into()
                }
                if let Some(v) = p.get("arch").and_then(Value::as_str) {
                    target.arch = v.into()
                }
                let plan = if let Some(loader) = p.get("forge_version").and_then(Value::as_str) {
                    svc.plan_forge(id, loader, source, target)
                } else if let Some(loader) = p.get("neoforge_version").and_then(Value::as_str) {
                    svc.plan_neoforge(id, loader, source, target)
                } else {
                    svc.plan_loader(
                        id,
                        p.get("fabric_loader").and_then(Value::as_str),
                        source,
                        target,
                    )
                };
                plan.map(|p| serde_json::to_value(p).unwrap())
            }
        }
    };
    let result = match result {
        Ok(v) => v,
        Err(e) => return cli_failure(method, p, e, err),
    };
    if json {
        #[derive(Serialize)]
        struct Document {
            protocol: u32,
            source: Source,
            result: Value,
        }
        serde_json::to_writer(
            &mut *out,
            &Document {
                protocol: 1,
                source,
                result,
            },
        )?;
        writeln!(out)?;
        return Ok(0);
    }
    if source == Source::Bmclapi {
        writeln!(out, "# source\tBMCLAPI")?;
    }
    match method {
        "version.get" => {
            for k in ["id", "type", "mainClass"] {
                writeln!(out, "{k}\t{}", result[k].as_str().unwrap())?
            }
            writeln!(
                out,
                "libraries\t{}",
                result["libraries"].as_array().unwrap().len()
            )?;
            writeln!(
                out,
                "client\t{}",
                result["downloads"]["client"]["url"].as_str().unwrap()
            )?;
            if !result["assetIndex"].is_null() {
                writeln!(
                    out,
                    "assetIndex\t{}",
                    result["assetIndex"]["id"].as_str().unwrap()
                )?
            }
            if !result["javaVersion"].is_null() {
                writeln!(
                    out,
                    "java\t{}\t{}",
                    result["javaVersion"]["majorVersion"],
                    result["javaVersion"]["component"].as_str().unwrap()
                )?
            }
            writeln!(
                out,
                "arguments\t{}",
                if result["arguments"].is_null() {
                    "legacy"
                } else {
                    "modern"
                }
            )?
        }
        _ => {
            writeln!(out, "version\t{}", result["version_id"].as_str().unwrap())?;
            writeln!(
                out,
                "target\t{}\t{}",
                result["target"]["os"].as_str().unwrap(),
                result["target"]["arch"].as_str().unwrap()
            )?;
            for (label, key) in [
                ("files", "files"),
                ("classpath", "classpath"),
                ("nativeExtractions", "native_extractions"),
                ("assetProjections", "asset_projections"),
            ] {
                writeln!(out, "{label}\t{}", result[key].as_array().unwrap().len())?;
            }
            writeln!(out, "downloadBytes\t{}", result["total_download_size"])?;
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::HashMap};
    struct Fake {
        responses: HashMap<String, Result<Vec<u8>, Error>>,
        calls: RefCell<Vec<String>>,
    }
    impl Transport for Fake {
        fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, Error> {
            self.calls.borrow_mut().push(url.into());
            self.responses
                .get(url)
                .cloned()
                .unwrap_or(Err(Error::Network))
        }
    }
    fn fixture() -> (Fake, Vec<u8>) {
        let data=br#"{"id":"demo","type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":12,"url":"https://piston-data.mojang.com/client"}},"javaVersion":{"component":"java-runtime","majorVersion":17}}"#.to_vec();
        let manifest=serde_json::json!({"latest":{"release":"demo","snapshot":"demo"},"versions":[{"id":"demo","type":"release","url":"https://piston-meta.mojang.com/demo","time":"now","releaseTime":"now","sha1":format!("{:x}",Sha1::digest(&data))}]}).to_string().into_bytes();
        let mut responses = HashMap::new();
        responses.insert(Source::Official.manifest_url().into(), Ok(manifest));
        responses.insert(
            "https://piston-meta.mojang.com/demo".into(),
            Ok(data.clone()),
        );
        (
            Fake {
                responses,
                calls: RefCell::new(vec![]),
            },
            data,
        )
    }
    #[test]
    fn cache_same_layout_and_stale_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, _) = fixture();
        let mut svc = Service::new(f, tmp.path());
        svc.now = 1000;
        svc.manifest(Source::Official).unwrap();
        let path = svc.cache_path(Source::Official.manifest_url());
        let env: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(env["v"], 1);
        assert_eq!(env["fetched_at"], 1000);
        svc.now = 1601;
        svc.transport.responses.clear();
        assert!(svc.manifest(Source::Official).is_ok());
        svc.now = 1000 + 86401;
        assert_eq!(svc.manifest(Source::Official), Err(Error::Network));
        assert!(!path.exists());
    }
    #[test]
    fn get_hash_and_id_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, _) = fixture();
        let svc = Service::new(f, tmp.path());
        assert_eq!(svc.get("demo", Source::Official).unwrap()["id"], "demo");
        assert_eq!(
            svc.get("missing", Source::Official),
            Err(Error::VersionNotFound)
        );
        svc.store("https://piston-meta.mojang.com/demo", b"corrupt");
        assert!(svc.get("demo", Source::Official).is_ok());
        assert_eq!(
            svc.transport
                .calls
                .borrow()
                .iter()
                .filter(|s| s.ends_with("/demo"))
                .count(),
            2
        );
    }
    #[test]
    fn mirror_fallback_keeps_requested_download_source() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, _) = fixture();
        let svc = Service::new(f, tmp.path());
        let response = handle_with(
            &svc,
            "install.plan",
            Some(&serde_json::json!({"id":"demo","source":"bmclapi","os":"linux","arch":"x86_64"})),
        )
        .unwrap();
        assert_eq!(response.source, Source::Bmclapi);
        assert_eq!(
            response.result["files"][0]["url"],
            "https://bmclapi2.bangbang93.com/client"
        );
    }
    #[test]
    fn invalid_requests_do_not_fetch() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, _) = fixture();
        let svc = Service::new(f, tmp.path());
        for (method, p) in [
            ("version.list", serde_json::json!({"limit":0})),
            ("version.get", serde_json::json!({"id":""})),
            (
                "version.resolve",
                serde_json::json!({"id":"x","fabric_loader":"a","forge_version":"b"}),
            ),
            ("install.plan", serde_json::json!({"id":"x","arch":"amd64"})),
        ] {
            assert_eq!(
                handle_with(&svc, method, Some(&p)).unwrap_err().code,
                "INVALID_PARAMS"
            );
        }
        assert!(svc.transport.calls.borrow().is_empty());
    }
}
#[cfg(test)]
mod fabric_transport_tests {
    use super::*;
    use std::cell::Cell;
    struct Head {
        calls: Cell<usize>,
    }
    impl Transport for Head {
        fn get(&self, _url: &str, _max: usize) -> Result<Vec<u8>, Error> {
            Err(Error::Network)
        }
        fn head_size(&self, _url: &str) -> Result<u64, Error> {
            self.calls.set(self.calls.get() + 1);
            Ok(123)
        }
    }
    #[test]
    fn head_size_is_cached_and_corrupt_value_evicted() {
        let root = tempfile::tempdir().unwrap();
        let s = Service::new(
            Head {
                calls: Cell::new(0),
            },
            root.path(),
        );
        let url = "https://example.invalid/lib.jar";
        s.store(&format!("head:{url}"), b"not an integer");
        assert_eq!(s.content_length(url), Ok(123));
        assert_eq!(s.content_length(url), Ok(123));
        assert_eq!(s.transport.calls.get(), 1);
    }
}
#[cfg(target_os = "linux")]
#[path = "version_server.rs"]
pub mod server;

#[cfg(test)]
#[path = "../tests/support/version_params_contract.rs"]
mod params_contract_tests;

#[cfg(all(test, unix))]
#[path = "version_listing_tests.rs"]
mod listing_tests;

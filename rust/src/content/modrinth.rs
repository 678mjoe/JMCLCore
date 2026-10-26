//! Typed Modrinth content metadata and deterministic selection, without credentials.
#[path = "modrinth_project.rs"]
pub(super) mod project_ordered;
#[path = "modrinth_version.rs"]
mod version_ordered;
use serde::{Deserialize, Serialize};
#[path = "modrinth_wire.rs"]
mod wire;
use wire::{WireProject, WireVersion};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    #[serde(deserialize_with = "string")]
    pub id: String,
    #[serde(deserialize_with = "string")]
    pub slug: String,
    #[serde(default)]
    pub title: Option<MetadataBytes>,
    #[serde(deserialize_with = "string")]
    pub project_type: String,
    #[serde(default)]
    #[serde(deserialize_with = "strings")]
    pub loaders: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Hashes {
    #[serde(deserialize_with = "string")]
    pub sha1: String,
    #[serde(default)]
    pub sha512: Option<MetadataBytes>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound(deserialize = "F: FilenameValue"))]
pub struct FileRecord<F> {
    #[serde(deserialize_with = "string")]
    pub url: String,
    #[serde(deserialize_with = "filename_value")]
    pub filename: F,
    #[serde(default)]
    pub primary: bool,
    pub hashes: Hashes,
    #[serde(deserialize_with = "number")]
    pub size: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Dependency {
    #[serde(default, deserialize_with = "optional_string")]
    pub version_id: Option<String>,
    #[serde(default, deserialize_with = "optional_string")]
    pub project_id: Option<String>,
    #[serde(default)]
    pub file_name: Option<MetadataBytes>,
    pub dependency_type: DependencyType,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound(deserialize = "F: FilenameValue"))]
pub struct VersionRecord<F> {
    #[serde(deserialize_with = "string")]
    pub id: String,
    #[serde(deserialize_with = "string")]
    pub project_id: String,
    pub version_number: MetadataBytes,
    #[serde(deserialize_with = "string")]
    pub version_type: String,
    #[serde(deserialize_with = "strings")]
    pub game_versions: Vec<String>,
    #[serde(deserialize_with = "strings")]
    pub loaders: Vec<String>,
    pub files: Vec<FileRecord<F>>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}
pub type File = FileRecord<String>;
pub type Version = VersionRecord<String>;
pub(super) trait FilenameValue: Sized {
    fn from_bytes(value: MetadataBytes) -> Result<Self>;
    fn filename_bytes(&self) -> &[u8];
}
impl FilenameValue for String {
    fn from_bytes(value: MetadataBytes) -> Result<Self> {
        String::from_utf8(value.into_bytes()).map_err(|_| "UnexpectedToken".into())
    }
    fn filename_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl FilenameValue for MetadataBytes {
    fn from_bytes(value: MetadataBytes) -> Result<Self> {
        Ok(value)
    }
    fn filename_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
pub(super) fn filename_value<'de, D: serde::Deserializer<'de>, F: FilenameValue>(
    d: D,
) -> std::result::Result<F, D::Error> {
    F::from_bytes(MetadataBytes::deserialize(d)?).map_err(|e| serde::de::Error::custom(e.0))
}
pub(super) fn number_width<'de, D: serde::Deserializer<'de>>(
    d: D,
    width: usize,
) -> std::result::Result<u64, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), width).map_err(|e| serde::de::Error::custom(e.name()))
}
pub(super) fn number<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
    number_width(d, 64)
}
fn ascii(s: &[u8], max: usize, extra: &[u8], err: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > max
        || !s
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(b))
    {
        Err(err.into())
    } else {
        Ok(())
    }
}
pub fn project_ref(s: &str) -> Result<()> {
    ascii(s.as_bytes(), 64, b"-_.", "InvalidModrinthProject")
}
pub fn version_id(s: &str) -> Result<()> {
    ascii(s.as_bytes(), 16, b"", "InvalidModrinthVersionId")
}
pub fn game_version(s: &str) -> Result<()> {
    ascii(s.as_bytes(), 64, b".-_+", "InvalidGameVersion")
}
fn loaders(v: &[MetadataBytes], required: bool) -> Result<()> {
    if v.len() > 32 || (required && v.is_empty()) {
        return Err("InvalidModrinthLoaders".into());
    }
    for s in v {
        ascii(s.as_bytes(), 32, b"-", "InvalidModrinthLoaders")?
    }
    Ok(())
}
pub fn parse_project(b: &[u8]) -> Result<Project> {
    let p = project_ordered::parse(b)?;
    ascii(p.id.as_bytes(), 64, b"-_.", "InvalidModrinthProject")?;
    ascii(p.slug.as_bytes(), 64, b"-_.", "InvalidModrinthProject")?;
    if !matches!(
        p.project_type.as_bytes(),
        b"mod" | b"resourcepack" | b"shader" | b"datapack"
    ) {
        return Err("UnsupportedProjectType".into());
    }
    loaders(&p.loaders, false)?;
    p.into_public()
}
fn rank(s: &str) -> Option<u8> {
    match s {
        "release" => Some(0),
        "beta" => Some(1),
        "alpha" => Some(2),
        _ => None,
    }
}
fn validate<F: FilenameValue>(v: &WireVersion<F>) -> Result<()> {
    ascii(v.id.as_bytes(), 16, b"", "InvalidModrinthVersionId")?;
    ascii(v.project_id.as_bytes(), 16, b"", "InvalidModrinthVersionId")?;
    if !matches!(v.version_type.as_bytes(), b"release" | b"beta" | b"alpha") {
        return Err("InvalidModrinthVersionType".into());
    }
    if v.game_versions.is_empty() || v.game_versions.len() > 128 {
        return Err("InvalidModrinthGameVersions".into());
    }
    for s in &v.game_versions {
        ascii(s.as_bytes(), 64, b".-_+", "InvalidGameVersion")?
    }
    loaders(&v.loaders, true)?;
    if v.files.is_empty() || v.files.len() > 64 {
        return Err("InvalidModrinthFiles".into());
    }
    for f in &v.files {
        let n = f.filename.filename_bytes();
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
            return Err("InvalidModrinthFilename".into());
        }
        if !f.url.starts_with("https://") {
            return Err("InsecureModrinthUrl".into());
        }
        if f.url.len() > 2048
            || f.url
                .bytes()
                .any(|b| b <= 32 || b == 127 || b == b'\\' || b == b'"')
        {
            return Err("InvalidModrinthUrl".into());
        }
        if f.hashes.sha1.as_bytes().len() != 40
            || !f
                .hashes
                .sha1
                .as_bytes()
                .iter()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err("InvalidModrinthHash".into());
        }
        if f.size == 0 || f.size > 512 * 1024 * 1024 {
            return Err("InvalidModrinthFileSize".into());
        }
    }
    if v.dependencies.len() > 128 {
        return Err("TooManyModrinthDependencies".into());
    }
    for d in &v.dependencies {
        if let Some(s) = &d.version_id {
            ascii(s.as_bytes(), 16, b"", "InvalidModrinthVersionId")?
        }
        if let Some(s) = &d.project_id {
            ascii(s.as_bytes(), 16, b"", "InvalidModrinthVersionId")?
        }
    }
    Ok(())
}
pub(super) fn parse_version_as<F: FilenameValue>(b: &[u8]) -> Result<VersionRecord<F>> {
    let v = version_ordered::one::<F>(b)?;
    validate(&v)?;
    v.into_public()
}
pub(super) fn parse_versions_as<F: FilenameValue>(b: &[u8]) -> Result<Vec<VersionRecord<F>>> {
    let v = version_ordered::many::<F>(b)?;
    if v.len() > 512 {
        return Err("TooManyModrinthVersions".into());
    }
    for x in &v {
        validate(x)?
    }
    v.into_iter().map(WireVersion::into_public).collect()
}
pub fn parse_version(b: &[u8]) -> Result<Version> {
    parse_version_as(b)
}
pub fn parse_versions(b: &[u8]) -> Result<Vec<Version>> {
    parse_versions_as(b)
}

pub fn supports<F>(v: &VersionRecord<F>, game: &str, loader: Option<&str>) -> bool {
    v.game_versions.iter().any(|s| s == game)
        && loader.is_none_or(|l| v.loaders.iter().any(|s| s == l))
}
pub fn select<'a, F>(
    v: &'a [VersionRecord<F>],
    game: &str,
    loader: Option<&str>,
) -> Option<&'a VersionRecord<F>> {
    v.iter()
        .filter(|v| supports(v, game, loader))
        .filter_map(|v| rank(&v.version_type).map(|r| (r, v)))
        .min_by_key(|(r, _)| *r)
        .map(|(_, v)| v)
}
pub(super) fn select_file_as<'a, F: FilenameValue>(
    v: &'a VersionRecord<F>,
    extension: Option<&str>,
) -> Option<&'a FileRecord<F>> {
    let mut fallback = None;
    let mut many = false;
    for f in &v.files {
        if extension.is_some_and(|e| !f.filename.filename_bytes().ends_with(e.as_bytes())) {
            continue;
        }
        if f.primary {
            return Some(f);
        }
        if fallback.is_some() {
            many = true
        } else {
            fallback = Some(f)
        }
    }
    if many { None } else { fallback }
}
pub fn select_file<'a>(v: &'a Version, extension: Option<&str>) -> Option<&'a File> {
    select_file_as(v, extension)
}

pub fn project_url(s: &str) -> Result<String> {
    project_ref(s)?;
    Ok(format!("https://api.modrinth.com/v2/project/{s}"))
}
pub fn version_url(s: &str) -> Result<String> {
    ascii(s.as_bytes(), 16, b"", "InvalidModrinthVersionId")?;
    Ok(format!("https://api.modrinth.com/v2/version/{s}"))
}
pub fn versions_url(s: &str, g: &str, l: Option<&str>) -> Result<String> {
    project_ref(s)?;
    game_version(g)?;
    let mut u =
        format!("https://api.modrinth.com/v2/project/{s}/version?game_versions=%5B%22{g}%22%5D");
    if let Some(l) = l {
        u.push_str(&format!("&loaders=%5B%22{l}%22%5D"))
    }
    Ok(u)
}

/// Byte-preserving metadata (including selection fields), not a filesystem path.
/// The original typed JSON schema emits UTF-8 as strings and other bytes as arrays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataBytes(Vec<u8>);
impl From<String> for MetadataBytes {
    fn from(v: String) -> Self {
        Self(v.into_bytes())
    }
}
impl From<Vec<u8>> for MetadataBytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}
impl MetadataBytes {
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}
impl Serialize for MetadataBytes {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => serializer.serialize_str(text),
            Err(_) => self.0.serialize(serializer),
        }
    }
}
impl<'de> Deserialize<'de> for MetadataBytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = MetadataBytes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("string or byte array")
            }
            fn visit_str<E: serde::de::Error>(
                self,
                s: &str,
            ) -> std::result::Result<MetadataBytes, E> {
                Ok(MetadataBytes(s.as_bytes().to_vec()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                s: String,
            ) -> std::result::Result<MetadataBytes, E> {
                Ok(MetadataBytes(s.into_bytes()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<MetadataBytes, A::Error> {
                // Zig []const u8 JSON arrays use typed u8 parsing for every
                // element, including quoted/integral-float numeric lexemes.
                // Do not round through Value/f64 or narrow serde's grammar.
                struct Byte(u8);
                impl<'de> Deserialize<'de> for Byte {
                    fn deserialize<D: serde::Deserializer<'de>>(
                        d: D,
                    ) -> std::result::Result<Self, D::Error> {
                        number_width(d, 8).map(|value| Self(value as u8))
                    }
                }
                let mut b = vec![];
                while let Some(Byte(v)) = a.next_element::<Byte>()? {
                    b.push(v)
                }
                Ok(MetadataBytes(b))
            }
        }
        d.deserialize_any(V)
    }
}
pub(super) fn string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<String, D::Error> {
    let bytes = MetadataBytes::deserialize(d)?;
    // Operational strings remain UTF-8-only until their separate path/validation
    // contracts are implemented. Never reinterpret or replace invalid bytes.
    String::from_utf8(bytes.0).map_err(|_| serde::de::Error::custom("InvalidUtf8"))
}
pub(super) fn optional_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct S(#[serde(deserialize_with = "string")] String);
    Option::<S>::deserialize(d).map(|s| s.map(|s| s.0))
}
pub(super) fn strings<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    struct S(#[serde(deserialize_with = "string")] String);
    Vec::<S>::deserialize(d).map(|s| s.into_iter().map(|s| s.0).collect())
}

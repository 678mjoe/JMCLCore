//! Typed Modrinth content metadata and deterministic selection, without credentials.
use serde::{Deserialize, Serialize};
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
pub struct File {
    #[serde(deserialize_with = "string")]
    pub url: String,
    #[serde(deserialize_with = "string")]
    pub filename: String,
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
pub struct Version {
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
    pub files: Vec<File>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
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
pub(super) fn parse<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    max: usize,
    array: bool,
) -> Result<T> {
    if bytes.len() > max {
        return Err("ModrinthResponseTooLarge".into());
    }
    if !bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == if array { b'[' } else { b'{' })
    {
        return Err("UnexpectedToken".into());
    }
    serde_json::from_slice(bytes).map_err(|e| {
        let s = e.to_string();
        Error(
            (if s.starts_with("missing field") {
                "MissingField"
            } else if s.starts_with("duplicate field") {
                "DuplicateField"
            } else if s.starts_with("unknown variant") {
                "InvalidEnumTag"
            } else if s.starts_with("Overflow") {
                "Overflow"
            } else if s.starts_with("InvalidNumber") {
                "InvalidNumber"
            } else if s.starts_with("InvalidCharacter") {
                "InvalidCharacter"
            } else if e.is_eof() {
                "UnexpectedEndOfInput"
            } else {
                "UnexpectedToken"
            })
            .into(),
        )
    })
}
fn ascii(s: &str, max: usize, extra: &[u8], err: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > max
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
    {
        Err(err.into())
    } else {
        Ok(())
    }
}
pub fn project_ref(s: &str) -> Result<()> {
    ascii(s, 64, b"-_.", "InvalidModrinthProject")
}
pub fn version_id(s: &str) -> Result<()> {
    ascii(s, 16, b"", "InvalidModrinthVersionId")
}
pub fn game_version(s: &str) -> Result<()> {
    ascii(s, 64, b".-_+", "InvalidGameVersion")
}
fn loaders(v: &[String], required: bool) -> Result<()> {
    if v.len() > 32 || (required && v.is_empty()) {
        return Err("InvalidModrinthLoaders".into());
    }
    for s in v {
        ascii(s, 32, b"-", "InvalidModrinthLoaders")?
    }
    Ok(())
}
pub fn parse_project(b: &[u8]) -> Result<Project> {
    let p: Project = parse(b, 1024 * 1024, false)?;
    project_ref(&p.id)?;
    project_ref(&p.slug)?;
    if !matches!(
        p.project_type.as_str(),
        "mod" | "resourcepack" | "shader" | "datapack"
    ) {
        return Err("UnsupportedProjectType".into());
    }
    loaders(&p.loaders, false)?;
    Ok(p)
}
fn rank(s: &str) -> Option<u8> {
    match s {
        "release" => Some(0),
        "beta" => Some(1),
        "alpha" => Some(2),
        _ => None,
    }
}
fn validate(v: &Version) -> Result<()> {
    version_id(&v.id)?;
    version_id(&v.project_id)?;
    if rank(&v.version_type).is_none() {
        return Err("InvalidModrinthVersionType".into());
    }
    if v.game_versions.is_empty() || v.game_versions.len() > 128 {
        return Err("InvalidModrinthGameVersions".into());
    }
    for s in &v.game_versions {
        game_version(s)?
    }
    loaders(&v.loaders, true)?;
    if v.files.is_empty() || v.files.len() > 64 {
        return Err("InvalidModrinthFiles".into());
    }
    for f in &v.files {
        let n = &f.filename;
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
        if f.hashes.sha1.len() != 40 || !f.hashes.sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
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
            version_id(s)?
        }
        if let Some(s) = &d.project_id {
            version_id(s)?
        }
    }
    Ok(())
}
pub fn parse_version(b: &[u8]) -> Result<Version> {
    let v = parse(b, 4 * 1024 * 1024, false)?;
    validate(&v)?;
    Ok(v)
}
pub fn parse_versions(b: &[u8]) -> Result<Vec<Version>> {
    let v: Vec<Version> = parse(b, 16 * 1024 * 1024, true)?;
    if v.len() > 512 {
        return Err("TooManyModrinthVersions".into());
    }
    for x in &v {
        validate(x)?
    }
    Ok(v)
}
pub fn supports(v: &Version, game: &str, loader: Option<&str>) -> bool {
    v.game_versions.iter().any(|s| s == game)
        && loader.is_none_or(|l| v.loaders.iter().any(|s| s == l))
}
pub fn select<'a>(v: &'a [Version], game: &str, loader: Option<&str>) -> Option<&'a Version> {
    v.iter()
        .filter(|v| supports(v, game, loader))
        .filter_map(|v| rank(&v.version_type).map(|r| (r, v)))
        .min_by_key(|(r, _)| *r)
        .map(|(_, v)| v)
}
pub fn select_file<'a>(v: &'a Version, extension: Option<&str>) -> Option<&'a File> {
    let mut fallback = None;
    let mut many = false;
    for f in &v.files {
        if extension.is_some_and(|e| !f.filename.ends_with(e)) {
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
pub fn project_url(s: &str) -> Result<String> {
    project_ref(s)?;
    Ok(format!("https://api.modrinth.com/v2/project/{s}"))
}
pub fn version_url(s: &str) -> Result<String> {
    version_id(s)?;
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

/// Byte-preserving supplemental metadata, not an operational filesystem path.
/// The original typed JSON schema emits UTF-8 as strings and other bytes as arrays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataBytes(Vec<u8>);
impl MetadataBytes {
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

use serde::{Deserialize, Serialize};
pub const MAX_BYTES: usize = 256 * 1024;
fn yes() -> bool {
    true
}
fn one() -> u32 {
    1
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Resourcepack,
    Shaderpack,
    Datapack,
}
impl Kind {
    pub fn directory(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Resourcepack => "resourcepacks",
            Self::Shaderpack => "shaderpacks",
            Self::Datapack => "datapacks",
        }
    }
    pub fn extension(self) -> &'static str {
        if self == Self::Mod { ".jar" } else { ".zip" }
    }
    pub fn from_directory(s: &str) -> Option<Self> {
        match s {
            "mods" => Some(Self::Mod),
            "resourcepacks" => Some(Self::Resourcepack),
            "shaderpacks" => Some(Self::Shaderpack),
            "datapacks" => Some(Self::Datapack),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Modrinth,
    Curseforge,
    Manual,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub provider: Provider,
    #[serde(default, deserialize_with = "super::modrinth::optional_string")]
    pub project_id: Option<String>,
    #[serde(default, deserialize_with = "super::modrinth::optional_string")]
    pub version_id: Option<String>,
    #[serde(default, deserialize_with = "super::modrinth::optional_string")]
    pub slug: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "F: RegistryFile"))]
pub struct EntryRecord<F: RegistryFile> {
    pub kind: Kind,
    #[serde(deserialize_with = "file")]
    pub file: F,
    #[serde(deserialize_with = "super::modrinth::string")]
    pub sha1: String,
    #[serde(deserialize_with = "number")]
    pub size: u64,
    pub source: Source,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub as_dependency: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "F: RegistryFile"))]
pub struct RegistryRecord<F: RegistryFile> {
    #[serde(default = "one", deserialize_with = "schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: Vec<EntryRecord<F>>,
}
impl<F: RegistryFile> Default for RegistryRecord<F> {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: vec![],
        }
    }
}
// Existing public mutation/storage types remain String-specialized.
pub type Entry = EntryRecord<String>;
pub type Registry = RegistryRecord<String>;
pub type ListingEntry = EntryRecord<super::ListedPath>;
pub(super) type ListingRegistry = RegistryRecord<super::ListedPath>;

pub trait RegistryFile: Clone + std::fmt::Debug + PartialEq + Eq + Serialize {
    fn file_bytes(&self) -> &[u8];
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error>;
}
impl RegistryFile for String {
    fn file_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        super::modrinth::string(d)
    }
}
impl RegistryFile for super::ListedPath {
    fn file_bytes(&self) -> &[u8] {
        &self.0
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        super::modrinth::MetadataBytes::deserialize(d).map(|v| Self(v.into_bytes()))
    }
}
fn file<'de, D: serde::Deserializer<'de>, F: RegistryFile>(d: D) -> Result<F, D::Error> {
    F::decode(d)
}
fn typed_number<'de, D: serde::Deserializer<'de>>(d: D, width: usize) -> Result<u64, D::Error> {
    // Zig content_registry.parse reads the disk document directly into Registry,
    // not through dynamic RPC Value. Keep its lexeme and typed numeric errors.
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), width)
        .map_err(|e| serde::de::Error::custom(format!("Zig:{}", e.name())))
}
fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    typed_number(d, 64)
}
fn schema<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    typed_number(d, 32).map(|n| n as u32)
}
pub fn parse(bytes: &[u8]) -> Result<Registry, &'static str> {
    parse_as(bytes)
}
pub(super) fn parse_listing(bytes: &[u8]) -> Result<ListingRegistry, &'static str> {
    parse_as(bytes)
}
fn parse_as<F: RegistryFile>(bytes: &[u8]) -> Result<RegistryRecord<F>, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("ContentRegistryTooLarge");
    }
    if !bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{')
    {
        return Err(if bytes.iter().all(u8::is_ascii_whitespace) {
            "UnexpectedEndOfInput"
        } else {
            "UnexpectedToken"
        });
    };
    let r: RegistryRecord<F> = serde_json::from_slice(bytes).map_err(|e| {
        let m = e.to_string();
        if m.starts_with("Zig:InvalidNumber") || m.starts_with("InvalidNumber") {
            "InvalidNumber"
        } else if m.starts_with("Zig:InvalidCharacter") || m.starts_with("InvalidCharacter") {
            "InvalidCharacter"
        } else if m.starts_with("Zig:Overflow") || m.starts_with("Overflow") {
            "Overflow"
        } else if e.is_eof() {
            "UnexpectedEndOfInput"
        } else if m.starts_with("missing field") {
            "MissingField"
        } else if m.starts_with("unknown variant") {
            "InvalidEnumTag"
        } else if m.starts_with("duplicate field") {
            "DuplicateField"
        } else {
            "UnexpectedToken"
        }
    })?;
    r.validate()?;
    Ok(r)
}
impl<F: RegistryFile> RegistryRecord<F> {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("UnsupportedContentSchema");
        }
        if self.entries.len() > 1024 {
            return Err("TooManyContentEntries");
        }
        for (i, e) in self.entries.iter().enumerate() {
            if !relative_path_bytes(e.file.file_bytes()) {
                return Err("UnsafePath");
            }
            if e.file.file_bytes().len() > 512 {
                return Err("InvalidContentFile");
            }
            let prefix = format!("{}/", e.kind.directory());
            if !e.file.file_bytes().starts_with(prefix.as_bytes())
                || e.file.file_bytes().len() <= prefix.len()
                || (e.kind == Kind::Datapack && !e.file.file_bytes().ends_with(b".zip"))
            {
                return Err("ContentKindPathMismatch");
            };
            if e.sha1.len() != 40 || !e.sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("InvalidContentSha1");
            };
            if e.size == 0 || e.size > 4 * 1024 * 1024 * 1024 {
                return Err("InvalidContentSize");
            };
            match e.source.provider {
                Provider::Manual => {
                    if e.source.project_id.is_some()
                        || e.source.version_id.is_some()
                        || e.source.slug.is_some()
                    {
                        return Err("InvalidContentSource");
                    }
                }
                p => {
                    let validate = if p == Provider::Modrinth {
                        version_id
                    } else {
                        numeric_id
                    };
                    // Match Zig's ordered validation: a missing ID is a
                    // content-source error, but a present malformed ID keeps
                    // the provider's error, even if the next ID is missing.
                    for value in [&e.source.project_id, &e.source.version_id] {
                        let value = value.as_deref().ok_or("InvalidContentSource")?;
                        if !validate(value) {
                            return Err(if p == Provider::Modrinth {
                                "InvalidModrinthVersionId"
                            } else {
                                "InvalidCurseforgeId"
                            });
                        }
                    }
                    if e.source.slug.as_deref().is_some_and(|s| !project_ref(s)) {
                        return Err("InvalidModrinthProject");
                    }
                }
            };
            for other in &self.entries[i + 1..] {
                if e.file == other.file {
                    return Err("DuplicateContentFile");
                }
                if e.source.provider == other.source.provider
                    && e.source.project_id.is_some()
                    && e.source.project_id == other.source.project_id
                {
                    return Err("DuplicateContentProject");
                }
            }
        }
        Ok(())
    }
}
pub fn project_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn version_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn numeric_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 10
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u32>().is_ok_and(|n| n > 0)
}
pub fn relative_path(p: &str) -> bool {
    relative_path_bytes(p.as_bytes())
}
fn relative_path_bytes(p: &[u8]) -> bool {
    !p.is_empty()
        && !p.contains(&b'\\')
        && p.split(|b| *b == b'/').all(|s| {
            if s.is_empty()
                || s == b"."
                || s == b".."
                || s.ends_with(b".")
                || s.ends_with(b" ")
                || s.iter().any(|b| *b < 32 || *b == 127 || *b == b':')
            {
                return false;
            };
            let stem = s.split(|b| *b == b'.').next().unwrap().to_ascii_uppercase();
            !matches!(
                stem.as_slice(),
                b"CON"
                    | b"PRN"
                    | b"AUX"
                    | b"NUL"
                    | b"COM1"
                    | b"COM2"
                    | b"COM3"
                    | b"COM4"
                    | b"COM5"
                    | b"COM6"
                    | b"COM7"
                    | b"COM8"
                    | b"COM9"
                    | b"LPT1"
                    | b"LPT2"
                    | b"LPT3"
                    | b"LPT4"
                    | b"LPT5"
                    | b"LPT6"
                    | b"LPT7"
                    | b"LPT8"
                    | b"LPT9"
            )
        })
}

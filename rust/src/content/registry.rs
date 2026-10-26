#[path = "registry_ordered.rs"]
mod ordered;
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
// Private persisted-schema enum coercion; public serde enum contracts stay named-only.
fn wire_enum_index(text: &str, names: &[&str]) -> Result<usize, &'static str> {
    if let Some(i) = names.iter().position(|name| *name == text) {
        return Ok(i);
    }
    if text.starts_with('-') || text.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
        return Err("InvalidEnumTag");
    }
    let n = crate::json_number::unsigned_text(text, 2).map_err(|_| "InvalidEnumTag")? as usize;
    if n >= names.len() {
        return Err("InvalidEnumTag");
    }
    Ok(n)
}
// Disk parsing retains every typed byte slice through full deserialization and
// ordered validation. Public storage/mutation structs still contain Strings.
struct WireSource {
    provider: Provider,
    project_id: Option<super::modrinth::MetadataBytes>,
    version_id: Option<super::modrinth::MetadataBytes>,
    slug: Option<super::modrinth::MetadataBytes>,
}
struct WireEntry<F: RegistryFile> {
    kind: Kind,
    file: F,
    sha1: super::modrinth::MetadataBytes,
    size: u64,
    source: WireSource,
    enabled: bool,
    as_dependency: bool,
}
struct WireRegistry<F: RegistryFile> {
    schema_version: u32,
    entries: Vec<WireEntry<F>>,
}
struct SourceView<'a> {
    provider: Provider,
    project_id: Option<&'a [u8]>,
    version_id: Option<&'a [u8]>,
    slug: Option<&'a [u8]>,
}
struct EntryView<'a> {
    kind: Kind,
    file: &'a [u8],
    sha1: &'a [u8],
    size: u64,
    source: SourceView<'a>,
}
impl<F: RegistryFile> WireRegistry<F> {
    fn into_validated(self) -> Result<RegistryRecord<F>, &'static str> {
        validate_header(self.schema_version, self.entries.len())?;
        let views: Vec<_> = self
            .entries
            .iter()
            .map(|e| EntryView {
                kind: e.kind,
                file: e.file.file_bytes(),
                sha1: e.sha1.as_bytes(),
                size: e.size,
                source: SourceView {
                    provider: e.source.provider,
                    project_id: e.source.project_id.as_ref().map(|s| s.as_bytes()),
                    version_id: e.source.version_id.as_ref().map(|s| s.as_bytes()),
                    slug: e.source.slug.as_ref().map(|s| s.as_bytes()),
                },
            })
            .collect();
        validate_entries(&views)?;
        // All present source strings and SHA-1 have now been proven ASCII.
        fn text(v: super::modrinth::MetadataBytes) -> Result<String, &'static str> {
            String::from_utf8(v.into_bytes()).map_err(|_| "UnexpectedToken")
        }
        let entries = self
            .entries
            .into_iter()
            .map(|e| {
                Ok(EntryRecord {
                    kind: e.kind,
                    file: e.file,
                    sha1: text(e.sha1)?,
                    size: e.size,
                    source: Source {
                        provider: e.source.provider,
                        project_id: e.source.project_id.map(text).transpose()?,
                        version_id: e.source.version_id.map(text).transpose()?,
                        slug: e.source.slug.map(text).transpose()?,
                    },
                    enabled: e.enabled,
                    as_dependency: e.as_dependency,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        Ok(RegistryRecord {
            schema_version: self.schema_version,
            entries,
        })
    }
}
pub fn parse(bytes: &[u8]) -> Result<Registry, &'static str> {
    parse_as(bytes)
}
pub(super) fn parse_listing(bytes: &[u8]) -> Result<ListingRegistry, &'static str> {
    parse_as(bytes)
}
fn parse_as<F: ordered::WireFile>(bytes: &[u8]) -> Result<RegistryRecord<F>, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("ContentRegistryTooLarge");
    }
    let r = ordered::parse::<F>(bytes)?;
    r.into_validated()
}
impl<F: RegistryFile> RegistryRecord<F> {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_header(self.schema_version, self.entries.len())?;
        let views: Vec<_> = self
            .entries
            .iter()
            .map(|e| EntryView {
                kind: e.kind,
                file: e.file.file_bytes(),
                sha1: e.sha1.as_bytes(),
                size: e.size,
                source: SourceView {
                    provider: e.source.provider,
                    project_id: e.source.project_id.as_deref().map(str::as_bytes),
                    version_id: e.source.version_id.as_deref().map(str::as_bytes),
                    slug: e.source.slug.as_deref().map(str::as_bytes),
                },
            })
            .collect();
        validate_entries(&views)
    }
}
fn validate_header(schema_version: u32, count: usize) -> Result<(), &'static str> {
    if schema_version != 1 {
        return Err("UnsupportedContentSchema");
    }
    if count > 1024 {
        return Err("TooManyContentEntries");
    }
    Ok(())
}
fn validate_entries(entries: &[EntryView<'_>]) -> Result<(), &'static str> {
    for (i, e) in entries.iter().enumerate() {
        if !relative_path_bytes(e.file) {
            return Err("UnsafePath");
        }
        if e.file.len() > 512 {
            return Err("InvalidContentFile");
        }
        let prefix = format!("{}/", e.kind.directory());
        if !e.file.starts_with(prefix.as_bytes())
            || e.file.len() <= prefix.len()
            || (e.kind == Kind::Datapack && !e.file.ends_with(b".zip"))
        {
            return Err("ContentKindPathMismatch");
        };
        if e.sha1.len() != 40 || !e.sha1.iter().all(|b| b.is_ascii_hexdigit()) {
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
                    version_id_bytes
                } else {
                    numeric_id_bytes
                };
                // Match Zig's ordered validation: a missing ID is a
                // content-source error, but a present malformed ID keeps
                // the provider's error, even if the next ID is missing.
                for value in [&e.source.project_id, &e.source.version_id] {
                    let value = value.ok_or("InvalidContentSource")?;
                    if !validate(value) {
                        return Err(if p == Provider::Modrinth {
                            "InvalidModrinthVersionId"
                        } else {
                            "InvalidCurseforgeId"
                        });
                    }
                }
                if e.source.slug.is_some_and(|s| !project_ref_bytes(s)) {
                    return Err("InvalidModrinthProject");
                }
            }
        };
        for other in &entries[i + 1..] {
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
pub fn project_ref(s: &str) -> bool {
    project_ref_bytes(s.as_bytes())
}
fn project_ref_bytes(s: &[u8]) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn version_id_bytes(s: &[u8]) -> bool {
    !s.is_empty() && s.len() <= 16 && s.iter().all(|b| b.is_ascii_alphanumeric())
}
fn numeric_id_bytes(s: &[u8]) -> bool {
    !s.is_empty()
        && s.len() <= 10
        && s.iter().all(u8::is_ascii_digit)
        && s.iter()
            .try_fold(0u32, |n, b| {
                n.checked_mul(10)?.checked_add(u32::from(*b - b'0'))
            })
            .is_some_and(|n| n > 0)
}
pub fn relative_path(p: &str) -> bool {
    relative_path_bytes(p.as_bytes())
}
pub(super) fn relative_path_bytes(p: &[u8]) -> bool {
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

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn listing_and_public_string_validation_share_source_rules() {
        for (provider, error) in [
            ("modrinth", "InvalidModrinthVersionId"),
            ("curseforge", "InvalidCurseforgeId"),
            ("manual", "InvalidContentSource"),
        ] {
            let bytes = serde_json::to_vec(&serde_json::json!({"entries":[{
                "kind":"mod", "file":[109,111,100,115,47,255,46,106,97,114],
                "sha1":"a".repeat(40), "size":1,
                "source":{"provider":provider,"project_id":[255],"version_id":"1"}
            }]}))
            .unwrap();
            assert_eq!(parse_listing(&bytes).unwrap_err(), error);
        }
        let bytes = br#"{"entries":[{"kind":"mod","file":"mods/a.jar","sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1,"source":{"provider":"modrinth","project_id":"Proj1","version_id":"Ver1"}}]}"#;
        let mut registry = parse(bytes).unwrap();
        registry.validate().unwrap();
        registry.entries[0].source.project_id = Some("bad/".into());
        assert_eq!(registry.validate(), Err("InvalidModrinthVersionId"));
        registry.entries[0].sha1 = "bad".into();
        assert_eq!(registry.validate(), Err("InvalidContentSha1"));
    }
}

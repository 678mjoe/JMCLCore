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
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    pub file: String,
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
pub struct Registry {
    #[serde(default = "one", deserialize_with = "schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: Vec<Entry>,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: vec![],
        }
    }
}
fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    let n = match v {
        serde_json::Value::Number(n) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|x| x.is_finite() && *x >= 0. && x.fract() == 0. && *x < u64::MAX as f64)
                .map(|x| x as u64)
        }),
        serde_json::Value::String(s) => s.parse::<u64>().ok().or_else(|| {
            s.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x >= 0. && x.fract() == 0. && *x < u64::MAX as f64)
                .map(|x| x as u64)
        }),
        _ => None,
    };
    n.ok_or_else(|| serde::de::Error::custom("invalid integer"))
}
fn schema<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    u32::try_from(number(d)?).map_err(serde::de::Error::custom)
}
pub fn parse(bytes: &[u8]) -> Result<Registry, &'static str> {
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
    let r: Registry = serde_json::from_slice(bytes).map_err(|e| {
        let m = e.to_string();
        if e.is_eof() {
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
impl Registry {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("UnsupportedContentSchema");
        }
        if self.entries.len() > 1024 {
            return Err("TooManyContentEntries");
        }
        for (i, e) in self.entries.iter().enumerate() {
            if !relative_path(&e.file) {
                return Err("UnsafePath");
            }
            if e.file.len() > 512 {
                return Err("InvalidContentFile");
            }
            let prefix = format!("{}/", e.kind.directory());
            if !e.file.starts_with(&prefix)
                || e.file.len() <= prefix.len()
                || (e.kind == Kind::Datapack && !e.file.ends_with(".zip"))
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
                    if !e.source.project_id.as_deref().is_some_and(validate)
                        || !e.source.version_id.as_deref().is_some_and(validate)
                        || e.source.slug.as_deref().is_some_and(|s| !project_ref(s))
                    {
                        return Err("InvalidContentSource");
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
    !p.is_empty()
        && !p.contains('\\')
        && p.split('/').all(|s| {
            if s.is_empty()
                || s == "."
                || s == ".."
                || s.ends_with(['.', ' '])
                || s.bytes().any(|b| b < 32 || b == 127 || b == b':')
            {
                return false;
            };
            let stem = s.split('.').next().unwrap().to_ascii_uppercase();
            !matches!(
                stem.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
        })
}

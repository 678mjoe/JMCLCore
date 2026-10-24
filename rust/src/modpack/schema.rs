use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl Error {
    pub fn name(&self) -> &str {
        &self.0
    }
}
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loader {
    Fabric,
    Forge,
    Neoforge,
}
impl Loader {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::Neoforge => "neoforge",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub loader: Loader,
    pub version: String,
}
fn number<'de, D: serde::Deserializer<'de>, const MAX: u64>(
    d: D,
) -> std::result::Result<u64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    crate::metadata::unsigned(&v, MAX).map_err(|e| serde::de::Error::custom(e.name()))
}
pub(super) fn u32_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u32, D::Error> {
    number::<D, { u32::MAX as u64 }>(d).map(|n| n as u32)
}
pub(super) fn u64_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u64, D::Error> {
    number::<D, { u64::MAX }>(d)
}
pub(super) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice::<Object<T>>(bytes)
        .map(|v| v.0)
        .map_err(|e| {
            let s = e.to_string();
            Error::from(if e.is_eof() {
                "UnexpectedEndOfInput"
            } else if s.starts_with("missing field") {
                "MissingField"
            } else if s.starts_with("duplicate field") {
                "DuplicateField"
            } else if s.starts_with("Overflow") {
                "Overflow"
            } else if s.starts_with("InvalidNumber") {
                "InvalidNumber"
            } else if s.starts_with("InvalidCharacter") {
                "InvalidCharacter"
            } else if s.starts_with("invalid type") || s.starts_with("UnexpectedToken") {
                "UnexpectedToken"
            } else {
                "SyntaxError"
            })
        })
}
pub fn relative_path(s: &str) -> Result<()> {
    relative_path_bytes(s.as_bytes())
}
pub fn relative_path_bytes(s: &[u8]) -> Result<()> {
    if s.is_empty() || s.contains(&b'\\') {
        return Err("UnsafePath".into());
    }
    for c in s.split(|b| *b == b'/') {
        if c.is_empty()
            || c == b"."
            || c == b".."
            || matches!(c.last(), Some(b'.' | b' '))
            || c.iter().any(|b| *b < 32 || *b == 127 || *b == b':')
        {
            return Err("UnsafePath".into());
        }
        let stem = c.split(|b| *b == b'.').next().unwrap();
        if [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .iter()
        .any(|s| stem.eq_ignore_ascii_case(s.as_bytes()))
        {
            return Err("UnsafePath".into());
        }
    }
    Ok(())
}

fn version(s: &str, error: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 64
        || s.bytes()
            .any(|b| !b.is_ascii_alphanumeric() && !b".-_+".contains(&b))
    {
        Err(error.into())
    } else {
        Ok(())
    }
}
fn name(s: &str, error: &str) -> Result<()> {
    if s.is_empty() || s.len() > 256 {
        Err(error.into())
    } else {
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Env {
    pub client: Option<String>,
    pub server: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Hashes {
    pub sha1: String,
    pub sha512: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub path: String,
    #[serde(deserialize_with = "object")]
    pub hashes: Hashes,
    #[serde(default, deserialize_with = "optional_object")]
    pub env: Option<Env>,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize", deserialize_with = "u64_number")]
    pub size: u64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Dependencies {
    pub minecraft: String,
    #[serde(rename = "fabric-loader")]
    pub fabric: Option<String>,
    #[serde(rename = "quilt-loader")]
    pub quilt: Option<String>,
    pub forge: Option<String>,
    pub neoforge: Option<String>,
}
impl Dependencies {
    pub fn loader(&self) -> Result<Option<Selection>> {
        if self.quilt.is_some() {
            return Err("UnsupportedModpackLoader".into());
        }
        let mut selection = None;
        for (loader, v) in [
            (Loader::Fabric, &self.fabric),
            (Loader::Forge, &self.forge),
            (Loader::Neoforge, &self.neoforge),
        ] {
            if let Some(v) = v {
                if selection.is_some() {
                    return Err("ModpackLoaderConflict".into());
                }
                version(v, "InvalidModpackLoaderVersion")?;
                selection = Some(Selection {
                    loader,
                    version: v.clone(),
                });
            }
        }
        Ok(selection)
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Mrpack {
    #[serde(rename = "formatVersion", deserialize_with = "u32_number")]
    pub format: u32,
    pub game: String,
    pub name: String,
    #[serde(rename = "versionId")]
    pub version: Option<String>,
    pub summary: Option<String>,
    #[serde(deserialize_with = "object_vec")]
    pub files: Vec<File>,
    #[serde(deserialize_with = "object")]
    pub dependencies: Dependencies,
}
impl Mrpack {
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > Self::MAX_BYTES {
            return Err("ModpackIndexTooLarge".into());
        }
        let v: Self = parse(bytes)?;
        v.validate()?;
        Ok(v)
    }
    pub fn validate(&self) -> Result<()> {
        if self.format != 1 {
            return Err("UnsupportedModpackFormat".into());
        }
        if self.game != "minecraft" {
            return Err("UnsupportedModpackGame".into());
        }
        name(&self.name, "InvalidModpackName")?;
        if let Some(v) = &self.version {
            name(v, "InvalidModpackVersion")?
        }
        self.dependencies.loader()?;
        version(&self.dependencies.minecraft, "InvalidGameVersion")?;
        if self.files.len() > 4096 {
            return Err("TooManyModpackFiles".into());
        }
        for (i, f) in self.files.iter().enumerate() {
            f.validate()?;
            if self.files[i + 1..].iter().any(|x| x.path == f.path) {
                return Err("DuplicateModpackFile".into());
            }
        }
        Ok(())
    }
}
impl File {
    pub fn ships_to_client(&self) -> bool {
        self.env.as_ref().and_then(|e| e.client.as_deref()) != Some("unsupported")
    }
    pub fn validate(&self) -> Result<()> {
        relative_path(&self.path)?;
        if self.path.len() > 512 {
            return Err("InvalidModpackFile".into());
        }
        for (s, n) in std::iter::once((&self.hashes.sha1, 40))
            .chain(self.hashes.sha512.as_ref().map(|s| (s, 128)))
        {
            if s.len() != n || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("InvalidModrinthHash".into());
            }
        }
        if self.size == 0 || self.size > 512 * 1024 * 1024 {
            return Err("InvalidModpackFileSize".into());
        }
        if self.downloads.is_empty() || self.downloads.len() > 8 {
            return Err("InvalidModpackDownloads".into());
        }
        for url in &self.downloads {
            if !url.starts_with("https://") {
                return Err("InsecureModpackUrl".into());
            }
            if url.len() > 2048
                || url
                    .bytes()
                    .any(|b| b <= 32 || b == 127 || b == b'\\' || b == b'"')
            {
                return Err("InvalidModpackUrl".into());
            }
        }
        if let Some(e) = &self.env {
            for v in [&e.client, &e.server].into_iter().flatten() {
                if !["required", "optional", "unsupported"].contains(&v.as_str()) {
                    return Err("InvalidModpackEnv".into());
                }
            }
        }
        Ok(())
    }
}
pub fn modrinth_ids(url: &str) -> Option<(&str, &str)> {
    let rest = url
        .strip_prefix("https://cdn.modrinth.com/data/")
        .or_else(|| url.strip_prefix("https://cdn-alt.modrinth.com/data/"))?;
    let (project, rest) = rest.split_once("/versions/")?;
    let (version, file) = rest.split_once('/')?;
    if file.is_empty()
        || [project, version]
            .iter()
            .any(|s| s.is_empty() || s.len() > 16 || !s.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        None
    } else {
        Some((project, version))
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct ModLoader {
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Minecraft {
    pub version: String,
    #[serde(default, rename = "modLoaders", deserialize_with = "object_vec")]
    pub loaders: Vec<ModLoader>,
}
impl Minecraft {
    pub fn loader(&self) -> Result<Option<Selection>> {
        let mut selected: Option<Selection> = None;
        for m in &self.loaders {
            if m.id.is_empty() || m.id.len() > 128 {
                return Err("InvalidModpackLoader".into());
            }
            let (kind, v) =
                m.id.split_once('-')
                    .ok_or(Error::from("InvalidModpackLoader"))?;
            if v.is_empty() {
                return Err("InvalidModpackLoader".into());
            }
            let loader = match kind {
                "fabric" => Loader::Fabric,
                "forge" => Loader::Forge,
                "neoforge" => Loader::Neoforge,
                _ => return Err("UnsupportedModpackLoader".into()),
            };
            let next = Selection {
                loader,
                version: v.into(),
            };
            if selected.as_ref().is_some_and(|s| s != &next) {
                return Err("ModpackLoaderConflict".into());
            }
            if m.primary {
                return Ok(Some(next));
            }
            selected = Some(next);
        }
        Ok(selected)
    }
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, Deserialize)]
pub struct Reference {
    #[serde(rename = "projectID", deserialize_with = "u32_number")]
    pub project: u32,
    #[serde(rename = "fileID", deserialize_with = "u32_number")]
    pub file: u32,
    #[serde(default = "yes")]
    pub required: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Curseforge {
    #[serde(deserialize_with = "object")]
    pub minecraft: Minecraft,
    #[serde(rename = "manifestType")]
    pub kind: String,
    #[serde(rename = "manifestVersion", deserialize_with = "u32_number")]
    pub format: u32,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    #[serde(deserialize_with = "object_vec")]
    pub files: Vec<Reference>,
    pub overrides: Option<String>,
}
impl Curseforge {
    pub const MAX_BYTES: usize = 4 * 1024 * 1024;
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > Self::MAX_BYTES {
            return Err("ModpackManifestTooLarge".into());
        }
        let v: Self = parse(bytes)?;
        v.validate()?;
        Ok(v)
    }
    pub fn validate(&self) -> Result<()> {
        if self.kind != "minecraftModpack" || self.format != 1 {
            return Err("UnsupportedModpackFormat".into());
        }
        name(&self.name, "InvalidModpackName")?;
        if let Some(v) = &self.version {
            name(v, "InvalidModpackVersion")?
        }
        version(&self.minecraft.version, "InvalidGameVersion")?;
        if self.minecraft.loaders.len() > 4 {
            return Err("InvalidModpackLoader".into());
        }
        self.minecraft.loader()?;
        if self.files.len() > 4096 {
            return Err("TooManyModpackFiles".into());
        }
        if self.files.iter().any(|f| f.project == 0 || f.file == 0) {
            return Err("InvalidCurseforgeId".into());
        }
        relative_path(self.overrides.as_deref().unwrap_or("overrides"))
    }
}

pub(super) fn u8_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u8, D::Error> {
    number::<D, 255>(d).map(|n| n as u8)
}

pub(super) struct Object<T>(pub T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = Object<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Object)
            }
        }
        d.deserialize_map(Visitor(std::marker::PhantomData))
    }
}
pub(super) fn object<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<T, D::Error> {
    Object::<T>::deserialize(d).map(|v| v.0)
}
pub(super) fn object_vec<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    Vec::<Object<T>>::deserialize(d).map(|v| v.into_iter().map(|v| v.0).collect())
}
pub(super) fn optional_object<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<Object<T>>::deserialize(d).map(|v| v.map(|v| v.0))
}

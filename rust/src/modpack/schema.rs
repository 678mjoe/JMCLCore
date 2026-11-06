use crate::content::modrinth::MetadataBytes;
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
    pub version: MetadataBytes,
}
fn number<'de, D: serde::Deserializer<'de>, const MAX: u64>(
    d: D,
) -> std::result::Result<u64, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    crate::metadata::unsigned(&v, MAX).map_err(|e| serde::de::Error::custom(e.name()))
}
// Schema documents are parsed from source bytes, so preserve the numeric token.
fn typed_number<'de, D: serde::Deserializer<'de>>(
    d: D,
    width: usize,
) -> std::result::Result<u64, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), width).map_err(|e| serde::de::Error::custom(e.name()))
}
pub(super) fn typed_u8_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u8, D::Error> {
    typed_number(d, 8).map(|n| n as u8)
}
pub(super) fn u32_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u32, D::Error> {
    typed_number(d, 32).map(|n| n as u32)
}
pub(super) fn u64_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<u64, D::Error> {
    typed_number(d, 64)
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

fn version_bytes(s: &[u8], error: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 64
        || s.iter()
            .any(|b| !b.is_ascii_alphanumeric() && !b".-_+".contains(b))
    {
        Err(error.into())
    } else {
        Ok(())
    }
}
fn name(s: &[u8], error: &str) -> Result<()> {
    if s.is_empty() || s.len() > 256 {
        Err(error.into())
    } else {
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Env {
    pub client: Option<MetadataBytes>,
    pub server: Option<MetadataBytes>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Hashes {
    pub sha1: MetadataBytes,
    pub sha512: Option<MetadataBytes>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub path: MetadataBytes,
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
    pub minecraft: MetadataBytes,
    #[serde(rename = "fabric-loader")]
    pub fabric: Option<MetadataBytes>,
    #[serde(rename = "quilt-loader")]
    pub quilt: Option<MetadataBytes>,
    pub forge: Option<MetadataBytes>,
    pub neoforge: Option<MetadataBytes>,
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
                version_bytes(v.as_bytes(), "InvalidModpackLoaderVersion")?;
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
    pub game: MetadataBytes,
    pub name: MetadataBytes,
    #[serde(rename = "versionId")]
    pub version: Option<MetadataBytes>,
    pub summary: Option<MetadataBytes>,
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
        if self.game.as_bytes() != b"minecraft" {
            return Err("UnsupportedModpackGame".into());
        }
        name(self.name.as_bytes(), "InvalidModpackName")?;
        if let Some(v) = &self.version {
            name(v.as_bytes(), "InvalidModpackVersion")?
        }
        self.dependencies.loader()?;
        version_bytes(self.dependencies.minecraft.as_bytes(), "InvalidGameVersion")?;
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
        self.env
            .as_ref()
            .and_then(|e| e.client.as_ref().map(MetadataBytes::as_bytes))
            != Some(b"unsupported")
    }
    pub fn validate(&self) -> Result<()> {
        relative_path_bytes(self.path.as_bytes())?;
        if self.path.as_bytes().len() > 512 {
            return Err("InvalidModpackFile".into());
        }
        for (s, n) in std::iter::once((&self.hashes.sha1, 40))
            .chain(self.hashes.sha512.as_ref().map(|s| (s, 128)))
        {
            if s.as_bytes().len() != n || !s.as_bytes().iter().all(|b| b.is_ascii_hexdigit()) {
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
                if ![b"required".as_slice(), b"optional", b"unsupported"].contains(&v.as_bytes()) {
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
    pub id: MetadataBytes,
    #[serde(default)]
    pub primary: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Minecraft {
    pub version: MetadataBytes,
    #[serde(default, rename = "modLoaders", deserialize_with = "object_vec")]
    pub loaders: Vec<ModLoader>,
}
impl Minecraft {
    pub fn loader(&self) -> Result<Option<Selection>> {
        let mut selected: Option<Selection> = None;
        for m in &self.loaders {
            let id = m.id.as_bytes();
            if id.is_empty() || id.len() > 128 {
                return Err("InvalidModpackLoader".into());
            }
            let separator = id
                .iter()
                .position(|b| *b == b'-')
                .ok_or(Error::from("InvalidModpackLoader"))?;
            let (kind, v) = (&id[..separator], &id[separator + 1..]);
            if v.is_empty() {
                return Err("InvalidModpackLoader".into());
            }
            let loader = match kind {
                b"fabric" => Loader::Fabric,
                b"forge" => Loader::Forge,
                b"neoforge" => Loader::Neoforge,
                _ => return Err("UnsupportedModpackLoader".into()),
            };
            let next = Selection {
                loader,
                version: v.to_vec().into(),
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
    pub kind: MetadataBytes,
    #[serde(rename = "manifestVersion", deserialize_with = "u32_number")]
    pub format: u32,
    pub name: MetadataBytes,
    pub version: Option<MetadataBytes>,
    pub author: Option<MetadataBytes>,
    #[serde(deserialize_with = "object_vec")]
    pub files: Vec<Reference>,
    pub overrides: Option<MetadataBytes>,
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
        if self.kind.as_bytes() != b"minecraftModpack" || self.format != 1 {
            return Err("UnsupportedModpackFormat".into());
        }
        name(self.name.as_bytes(), "InvalidModpackName")?;
        if let Some(v) = &self.version {
            name(v.as_bytes(), "InvalidModpackVersion")?
        }
        version_bytes(self.minecraft.version.as_bytes(), "InvalidGameVersion")?;
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
        relative_path_bytes(
            self.overrides
                .as_ref()
                .map(MetadataBytes::as_bytes)
                .unwrap_or(b"overrides"),
        )
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

#[cfg(test)]
mod typed_numeric_tests {
    use super::*;
    #[derive(Deserialize)]
    struct Narrow {
        #[serde(deserialize_with = "u32_number")]
        n: u32,
    }
    #[derive(Deserialize)]
    struct Wide {
        #[serde(deserialize_with = "u64_number")]
        n: u64,
    }
    #[test]
    fn raw_schema_numbers_do_not_round_through_binary64() {
        assert!(parse::<Narrow>(br#"{"n":1.0000000000000001}"#).is_err());
        assert_eq!(
            parse::<Narrow>(br#"{"n":1.00000000000000000000000000000000001}"#)
                .unwrap()
                .n,
            1
        );
        assert_eq!(
            parse::<Wide>(br#"{"n":9007199254740993.0}"#).unwrap().n,
            9007199254740993
        );
        assert_eq!(
            parse::<Wide>(br#"{"n":18446744073709551615.0}"#).unwrap().n,
            u64::MAX
        );
        assert!(parse::<Wide>(br#"{"n":18446744073709551615.1}"#).is_err());
    }
}

#[cfg(test)]
mod numeric_wrapper_tests {
    use super::*;
    #[derive(Deserialize)]
    struct Typed {
        #[serde(deserialize_with = "typed_u8_number")]
        n: u8,
    }
    #[derive(Deserialize)]
    struct Dynamic {
        #[serde(deserialize_with = "u8_number")]
        n: u8,
    }
    #[derive(Deserialize)]
    struct Wrapped {
        #[serde(deserialize_with = "optional_object")]
        one: Option<Typed>,
        #[serde(deserialize_with = "object_vec")]
        many: Vec<Typed>,
    }
    #[test]
    fn object_wrappers_retain_tokens_and_rpc_retains_dynamic_numbers() {
        let w: Wrapped = parse(br#"{"one":{"n":255.0},"many":[{"n":"2_54"}]}"#).unwrap();
        assert_eq!(w.one.unwrap().n, 255);
        assert_eq!(w.many[0].n, 254);
        assert!(parse::<Wrapped>(br#"{"one":{"n":1.0000000000000001},"many":[]}"#).is_err());
        assert!(parse::<Wrapped>(br#"{"one":null,"many":[{"n":1.0000000000000001}]}"#).is_err());
        assert!(parse::<Typed>(br#"{"n":256}"#).is_err());
        assert_eq!(
            parse::<Dynamic>(br#"{"n":3.0000000000000001}"#).unwrap().n,
            3
        );
        assert!(parse::<Dynamic>(br#"{"n":"3.0000000000000001"}"#).is_err());
    }
}

#[cfg(test)]
mod env_byte_tests {
    use super::*;
    fn fixture(client: &str, server: &str, size: u8) -> Vec<u8> {
        format!(r#"{{"path":"mods/a.jar","hashes":{{"sha1":"1111111111111111111111111111111111111111"}},"env":{{"client":{client},"server":{server}}},"downloads":["https://example.com/a.jar"],"fileSize":{size}}}"#).into_bytes()
    }
    #[test]
    fn typed_environment_values_keep_validation_and_client_selection() {
        for (text, ships) in [
            ("required", true),
            ("optional", true),
            ("unsupported", false),
        ] {
            let bytes = format!("{:?}", text.as_bytes());
            let file: File = parse(&fixture(&bytes, &bytes, 1)).unwrap();
            file.validate().unwrap();
            assert_eq!(file.ships_to_client(), ships);
        }
        let file: File = parse(&fixture("null", "null", 1)).unwrap();
        file.validate().unwrap();
        assert!(file.ships_to_client());
        for value in ["[]", "[255]", "[0]", r#""other""#] {
            let file: File = parse(&fixture(value, "null", 1)).unwrap();
            assert_eq!(file.validate().unwrap_err().name(), "InvalidModpackEnv");
            let file: File = parse(&fixture(value, "null", 0)).unwrap();
            assert_eq!(
                file.validate().unwrap_err().name(),
                "InvalidModpackFileSize"
            );
        }
    }
    #[test]
    fn environment_bytes_survive_complete_index_validation() {
        let entry = String::from_utf8(fixture(
            "[117,110,115,117,112,112,111,114,116,101,100]",
            "null",
            1,
        ))
        .unwrap();
        let raw = format!(
            r#"{{"formatVersion":1,"game":"minecraft","name":"fixture","dependencies":{{"minecraft":"1.20.1"}},"files":[{entry}]}}"#
        );
        let index = Mrpack::parse(raw.as_bytes()).unwrap();
        assert!(!index.files[0].ships_to_client());
        for value in ["[256]", "[65.5]", "{}"] {
            assert!(parse::<File>(&fixture(value, "null", 1)).is_err());
        }
    }
}

#[cfg(test)]
mod hash_byte_tests {
    use super::*;
    fn fixture(sha1: &str, sha512: &str, path: &str, size: u8, env: &str) -> Vec<u8> {
        format!(r#"{{"formatVersion":1,"game":"minecraft","name":"fixture","dependencies":{{"minecraft":"1.20.1"}},"files":[{{"path":"{path}","hashes":{{"sha1":{sha1},"sha512":{sha512}}},"env":{{"client":"{env}"}},"downloads":["https://cdn.modrinth.com/data/A/versions/B/a.jar"],"fileSize":{size}}}]}}"#).into_bytes()
    }
    #[test]
    fn typed_hashes_reach_both_plan_outputs_without_loss() {
        let a = format!("{:?}", vec![65; 40]);
        let b = format!("{:?}", vec![102; 128]);
        let pack = Mrpack::parse(&fixture(&a, &b, "mods/a.jar", 1, "required")).unwrap();
        let plan = crate::modpack::planning::Downloads::mrpack(&pack);
        assert_eq!(plan.files[0].sha1, "A".repeat(40));
        assert_eq!(plan.entries[0].sha1, "A".repeat(40));
        let pack = Mrpack::parse(&fixture(&a, &b, "mods/a.jar", 1, "unsupported")).unwrap();
        let plan = crate::modpack::planning::Downloads::mrpack(&pack);
        assert_eq!(plan.skipped, 1);
        assert!(plan.files.is_empty() && plan.entries.is_empty());
    }
    #[test]
    fn hash_validation_keeps_path_hash_size_and_server_only_order() {
        for sha in ["[]", "[255]", &format!("{:?}", vec![103; 40])] {
            assert_eq!(
                Mrpack::parse(&fixture(sha, "null", "mods/a.jar", 0, "unsupported"))
                    .unwrap_err()
                    .name(),
                "InvalidModrinthHash"
            );
            assert_eq!(
                Mrpack::parse(&fixture(sha, "null", "../a.jar", 0, "required"))
                    .unwrap_err()
                    .name(),
                "UnsafePath"
            );
        }
        let valid = format!("{:?}", vec![49; 40]);
        assert_eq!(
            Mrpack::parse(&fixture(&valid, "[255]", "mods/a.jar", 0, "required"))
                .unwrap_err()
                .name(),
            "InvalidModrinthHash"
        );
        for sha in ["[256]", "[65.5]", "{}"] {
            assert!(Mrpack::parse(&fixture(sha, "null", "mods/a.jar", 1, "required")).is_err());
        }
    }
}

#[cfg(test)]
mod dependency_byte_tests {
    use super::*;
    fn fixture(deps: &str) -> Vec<u8> {
        format!(r#"{{"formatVersion":1,"game":"minecraft","name":"fixture","dependencies":{deps},"files":[]}}"#).into_bytes()
    }
    #[test]
    fn typed_dependency_bytes_reach_identity_and_loader() {
        for field in ["fabric-loader", "forge", "neoforge"] {
            let deps = format!(r#"{{"minecraft":[49,46,50,48,46,49],"{field}":[49,46,50]}}"#);
            let pack =
                crate::modpack::planning::Pack::Modrinth(Mrpack::parse(&fixture(&deps)).unwrap());
            let (_, game, loader) = pack.identity().unwrap();
            assert_eq!(game, "1.20.1");
            assert_eq!(loader.unwrap().version.as_bytes(), b"1.2");
        }
    }
    #[test]
    fn dependency_validation_priority_stays_unchanged() {
        for (deps, error) in [
            (
                r#"{"minecraft":[255],"quilt-loader":[255]}"#,
                "UnsupportedModpackLoader",
            ),
            (
                r#"{"minecraft":[255],"fabric-loader":[255],"forge":"1"}"#,
                "InvalidModpackLoaderVersion",
            ),
            (
                r#"{"minecraft":[255],"fabric-loader":"1","forge":[255]}"#,
                "ModpackLoaderConflict",
            ),
            (r#"{"minecraft":[255]}"#, "InvalidGameVersion"),
            (r#"{"minecraft":[]}"#, "InvalidGameVersion"),
        ] {
            assert_eq!(Mrpack::parse(&fixture(deps)).unwrap_err().name(), error);
        }
        for deps in [
            r#"{"minecraft":[256]}"#,
            r#"{"minecraft":"1","fabric-loader":[65.5]}"#,
        ] {
            assert!(Mrpack::parse(&fixture(deps)).is_err());
        }
    }
}

#[cfg(test)]
mod kind_byte_tests {
    use super::*;
    #[test]
    fn typed_pack_discriminators_keep_original_precedence() {
        let game = format!("{:?}", b"minecraft");
        let raw = format!(
            r#"{{"formatVersion":1,"game":{game},"name":"fixture","dependencies":{{"minecraft":"1.20.1"}},"files":[]}}"#
        );
        Mrpack::parse(raw.as_bytes()).unwrap();
        let bad = raw.replace(&game, "[255]");
        assert_eq!(
            Mrpack::parse(bad.as_bytes()).unwrap_err().name(),
            "UnsupportedModpackGame"
        );
        let bad = bad.replace(r#""formatVersion":1"#, r#""formatVersion":2"#);
        assert_eq!(
            Mrpack::parse(bad.as_bytes()).unwrap_err().name(),
            "UnsupportedModpackFormat"
        );
        let kind = format!("{:?}", b"minecraftModpack");
        let raw = format!(
            r#"{{"manifestVersion":1,"manifestType":{kind},"name":"fixture","minecraft":{{"version":"1.20.1"}},"files":[]}}"#
        );
        Curseforge::parse(raw.as_bytes()).unwrap();
        let bad = raw.replace(&kind, "[255]").replace("fixture", "");
        assert_eq!(
            Curseforge::parse(bad.as_bytes()).unwrap_err().name(),
            "UnsupportedModpackFormat"
        );
    }
}

#[cfg(test)]
mod cf_game_byte_tests {
    use super::*;
    fn fixture(version: &str, loaders: &str) -> Vec<u8> {
        format!(r#"{{"manifestVersion":1,"manifestType":"minecraftModpack","name":"fixture","minecraft":{{"version":{version},"modLoaders":{loaders}}},"files":[]}}"#).into_bytes()
    }
    #[test]
    fn curseforge_game_version_bytes_reach_identity() {
        let pack = crate::modpack::planning::Pack::Curseforge(
            Curseforge::parse(&fixture("[49,46,50,48,46,49]", "[]")).unwrap(),
        );
        assert_eq!(pack.identity().unwrap().1, "1.20.1");
        for version in ["[]", "[255]", &format!("{:?}", vec![65; 65])] {
            for loaders in [
                r#"[{"id":"invalid"}]"#,
                r#"[{"id":""},{"id":""},{"id":""},{"id":""},{"id":""}]"#,
            ] {
                assert_eq!(
                    Curseforge::parse(&fixture(version, loaders))
                        .unwrap_err()
                        .name(),
                    "InvalidGameVersion"
                );
            }
        }
        for version in ["[256]", "[65.5]", "{}"] {
            assert!(Curseforge::parse(&fixture(version, "[]")).is_err());
        }
    }
}

#[cfg(test)]
mod rebuilt_cf_byte_tests {
    use super::*;
    fn fixture(loaders: serde_json::Value, overrides: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","files":[],"minecraft":{"version":"1.20.1","modLoaders":loaders},"overrides":overrides})).unwrap()
    }
    #[test]
    fn typed_loader_suffix_and_override_directory_preserve_bytes() {
        for prefix in ["fabric", "forge", "neoforge"] {
            for suffix in [
                vec![255, 0, b'-', 254],
                vec![0],
                "版".as_bytes().to_vec(),
                vec![b'x'; 128 - prefix.len() - 1],
            ] {
                let mut id = format!("{prefix}-").into_bytes();
                id.extend(&suffix);
                let p = Curseforge::parse(&fixture(
                    serde_json::json!([{"id":id,"primary":true},{"id":"quilt-invalid"}]),
                    serde_json::json!([255, 47, 254]),
                ))
                .unwrap();
                let s = p.minecraft.loader().unwrap().unwrap();
                assert_eq!(s.loader.name(), prefix);
                let expected = match std::str::from_utf8(&suffix) {
                    Ok(v) => serde_json::json!(v),
                    Err(_) => serde_json::json!(suffix),
                };
                assert_eq!(serde_json::to_value(s.version).unwrap(), expected);
                assert_eq!(
                    serde_json::to_value(p.overrides).unwrap(),
                    serde_json::json!([255, 47, 254])
                );
            }
        }
    }
}

#[cfg(test)]
mod rebuilt_cf_order_tests {
    use super::*;
    fn document(loaders: &str, overrides: &str, files: &str) -> Vec<u8> {
        format!(r#"{{"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","files":{files},"minecraft":{{"version":"1.20.1","modLoaders":{loaders}}},"overrides":{overrides}}}"#).into_bytes()
    }
    #[test]
    fn loader_and_override_errors_keep_original_order_and_bounds() {
        for (loaders, overrides, files, error) in [
            (
                r#"[{"id":[102,97,98,114,105,99,45,255]},{"id":"fabric-x","primary":true}]"#,
                "null",
                "[]",
                "ModpackLoaderConflict",
            ),
            (
                r#"[{"id":[102,97,98,114,105,99,45,256]}]"#,
                "null",
                "[]",
                "Overflow",
            ),
            (
                r#"[{"id":[102,97,98,114,105,99,45,65.00000000000000001]}]"#,
                "null",
                "[]",
                "InvalidNumber",
            ),
            (
                r#"[{"id":[255,45,1]}]"#,
                "null",
                "[]",
                "UnsupportedModpackLoader",
            ),
            (
                r#"[{"id":[102,97,98,114,105,99,45]}]"#,
                "null",
                "[]",
                "InvalidModpackLoader",
            ),
            ("[]", "[256]", "[]", "Overflow"),
            ("[]", "[65.00000000000000001]", "[]", "InvalidNumber"),
            ("[]", "[]", "[]", "UnsafePath"),
            ("[]", "[255,0]", "[]", "UnsafePath"),
            ("[]", "[46,46,47,255]", "[]", "UnsafePath"),
            ("[]", "[255,47,46]", "[]", "UnsafePath"),
            ("[]", "[255,92,254]", "[]", "UnsafePath"),
            (
                "[]",
                "[]",
                r#"[{"projectID":0,"fileID":1}]"#,
                "InvalidCurseforgeId",
            ),
            (
                r#"[{"id":"invalid"}]"#,
                "[]",
                r#"[{"projectID":0,"fileID":1}]"#,
                "InvalidModpackLoader",
            ),
        ] {
            assert_eq!(
                Curseforge::parse(&document(loaders, overrides, files))
                    .unwrap_err()
                    .name(),
                error
            );
        }
        let same = r#"[{"id":[102,97,98,114,105,99,45,255]},{"id":[102,97,98,114,105,99,45,255],"primary":true},{"id":"quilt-x"}]"#;
        assert_eq!(
            Curseforge::parse(&document(same, "null", "[]"))
                .unwrap()
                .minecraft
                .loader()
                .unwrap()
                .unwrap()
                .version
                .as_bytes(),
            &[255]
        );
        let too_long = serde_json::json!([{"id":format!("fabric-{}","x".repeat(122))}]);
        assert_eq!(
            Curseforge::parse(&document(&too_long.to_string(), "null", "[]"))
                .unwrap_err()
                .name(),
            "InvalidModpackLoader"
        );
        let too_many = serde_json::json!([{"id":"fabric-x","primary":true},{"id":"fabric-x"},{"id":"fabric-x"},{"id":"fabric-x"},{"id":"fabric-x"}]);
        assert_eq!(
            Curseforge::parse(&document(&too_many.to_string(), "null", "[]"))
                .unwrap_err()
                .name(),
            "InvalidModpackLoader"
        );
        // No unrequested limit is added to the validated override directory itself.
        let long_dir = serde_json::to_string(&vec![255; 1024]).unwrap();
        assert!(Curseforge::parse(&document("[]", &long_dir, "[]")).is_ok());
    }
}

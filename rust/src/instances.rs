//! Pinned instance schema and offline JSONL operations. Network validation remains explicit.
use crate::protocol::RpcError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
mod cli;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::Root;
#[cfg(windows)]
mod windows;
pub use cli::run as cli;
pub(crate) use cli::{game_path as minecraft_path, zig_error as cli_error_name};
#[cfg(windows)]
pub(crate) use windows::Root;

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
fn one() -> u32 {
    1
}
fn official() -> String {
    "official".into()
}
fn yes() -> bool {
    true
}
// Zig's integer schema parser also accepts integral JSON floats/numeric strings.
pub(crate) fn schema_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), 32)
        .map(|n| n as u32)
        .map_err(|e| serde::de::Error::custom(format!("Zig:{}", e.name())))
}

// Only dynamic Value callers may use this visitor: finite JSON floats have
// already been rounded to binary64 by the original request params parser.
pub(crate) fn dynamic_schema_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<u32, D::Error> {
    struct N;
    impl<'de> serde::de::Visitor<'de> for N {
        type Value = u32;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("u32-compatible number")
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<u32, E> {
            u32::try_from(v).map_err(|_| E::custom("Zig:Overflow"))
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<u32, E> {
            u32::try_from(v).map_err(|_| E::custom("Zig:Overflow"))
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<u32, E> {
            if !v.is_finite() || v.fract() != 0.0 {
                return Err(E::custom("Zig:InvalidNumber"));
            }
            if v < 0.0 || v > u32::MAX as f64 {
                return Err(E::custom("Zig:Overflow"));
            }
            Ok(v as u32)
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<u32, E> {
            crate::json_number::unsigned_text(v, 32)
                .map(|n| n as u32)
                .map_err(|e| E::custom(format!("Zig:{}", e.name())))
        }
    }
    d.deserialize_any(N)
}
pub(crate) fn parse_document<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, &'static str> {
    fn error(e: serde_json::Error) -> &'static str {
        let message = e.to_string();
        if message.contains("Zig:Overflow") {
            "Overflow"
        } else if message.contains("Zig:InvalidNumber") {
            "InvalidNumber"
        } else if message.contains("Zig:InvalidCharacter") {
            "InvalidCharacter"
        } else if e.is_eof() {
            "UnexpectedEndOfInput"
        } else if e.is_syntax() {
            "SyntaxError"
        } else if message.starts_with("missing field") {
            "MissingField"
        } else if message.starts_with("duplicate field") {
            "DuplicateField"
        } else {
            "UnexpectedToken"
        }
    }
    // Shape/syntax validation must not round or reject numeric lexemes before
    // the typed schema parser sees them (including ignored huge exponents).
    let raw: &RawValue = serde_json::from_slice(bytes).map_err(error)?;
    if crate::auth::json::Json::parse(raw.get()).is_err() {
        return Err("SyntaxError");
    }
    if !raw.get().starts_with('{') {
        return Err("UnexpectedToken");
    }
    serde_json::from_slice(bytes).map_err(error)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    #[serde(default = "one", deserialize_with = "schema_number")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version_id: String,
    #[serde(default)]
    pub fabric_loader: Option<String>,
    #[serde(default)]
    pub neoforge_version: Option<String>,
    #[serde(default)]
    pub forge_version: Option<String>,
    #[serde(default = "official")]
    pub source: String,
}
impl Manifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_id(&self.id)?;
        if self.schema_version != 1 {
            return Err("UnsupportedInstanceSchema");
        }
        if self.name.is_empty() || self.name.len() > 256 {
            return Err("InvalidInstanceName");
        }
        if self.version_id.is_empty() || self.version_id.len() > 256 {
            return Err("InvalidVersionId");
        }
        for (v, err) in [
            (&self.fabric_loader, "InvalidFabricLoaderVersion"),
            (&self.neoforge_version, "InvalidNeoForgeVersion"),
            (&self.forge_version, "InvalidForgeVersion"),
        ] {
            if let Some(v) = v
                && (v.is_empty() || v.len() > 256)
            {
                return Err(err);
            }
        }
        if [
            &self.fabric_loader,
            &self.neoforge_version,
            &self.forge_version,
        ]
        .iter()
        .filter(|s| s.is_some())
        .count()
            > 1
        {
            return Err("LoaderConflict");
        }
        if !matches!(self.source.as_str(), "official" | "bmclapi") {
            return Err("InvalidEnumTag");
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], id: &str) -> Result<Self, &'static str> {
        validate_id(id)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err("InstanceManifestTooLarge");
        }
        let m: Self = parse_document(bytes)?;
        m.validate()?;
        if m.id != id {
            return Err("InstanceIdMismatch");
        }
        Ok(m)
    }
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        let mut b = serde_json::to_vec(self).map_err(|_| "InvalidInstanceManifest")?;
        b.push(b'\n');
        Ok(b)
    }
    pub fn fields(&self, installed: bool) -> Value {
        json!({"id":self.id,"name":self.name,"version_id":self.version_id,"fabric_loader":self.fabric_loader,"neoforge_version":self.neoforge_version,"forge_version":self.forge_version,"source":self.source,"installed":installed})
    }
}
pub fn validate_id(id: &str) -> Result<(), &'static str> {
    let b = id.as_bytes();
    let alnum = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    if b.is_empty()
        || b.len() > 64
        || !alnum(b[0])
        || b[1..]
            .iter()
            .any(|c| !alnum(*c) && !matches!(*c, b'.' | b'_' | b'-'))
        || id.ends_with('.')
    {
        return Err("InvalidInstanceId");
    }
    let stem = id.split('.').next().unwrap();
    let sb = stem.as_bytes();
    if matches!(stem, "con" | "prn" | "aux" | "nul")
        || (sb.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && (b'1'..=b'9').contains(&sb[3]))
    {
        return Err("InvalidInstanceId");
    }
    Ok(())
}
#[derive(Deserialize)]
struct Directory {
    directory: String,
}
#[derive(Deserialize)]
struct Target {
    directory: String,
    id: String,
}
#[derive(Deserialize)]
struct Create {
    directory: String,
    id: String,
    version_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    fabric_loader: Option<String>,
    #[serde(default)]
    neoforge_version: Option<String>,
    #[serde(default)]
    forge_version: Option<String>,
    #[serde(default = "official")]
    source: String,
    #[serde(default = "yes")]
    validate: bool,
}
/// Resolve before opening the instances root, matching Service.resolveMetadataPreferred.
/// Validation never installs artifacts or creates an instance directory.
fn validate_selection(
    id: &str,
    fabric: Option<&str>,
    neoforge: Option<&str>,
    forge: Option<&str>,
    source: &str,
) -> Result<(), crate::metadata::Error> {
    use crate::{
        metadata::Source,
        version::{HttpTransport, Service},
    };
    let source = Source::parse(source).ok_or(crate::metadata::Error::Plan("InvalidSource"))?;
    let service = Service::new(HttpTransport::new()?, ".jmclcore/metadata-cache");
    if let Some(loader) = neoforge {
        service
            .resolve_neoforge_preferred(id, loader, source)
            .map(|_| ())
    } else if let Some(loader) = forge {
        service
            .resolve_forge_preferred(id, loader, source)
            .map(|_| ())
    } else {
        service.resolve_preferred(id, fabric, source).map(|_| ())
    }
}
fn resolution_error(error: crate::metadata::Error) -> RpcError {
    if error == crate::metadata::Error::MetadataFetchTimeout {
        fail(
            "METADATA_TIMEOUT",
            "Timed out while resolving the Minecraft version and loader",
        )
    } else {
        fail(
            "VERSION_RESOLUTION_FAILED",
            "Failed to resolve the Minecraft version and loader",
        )
    }
}
fn invalid(message: &str) -> RpcError {
    RpcError::new("INVALID_PARAMS", message)
}
fn fail(code: &str, message: &str) -> RpcError {
    RpcError::new(code, message)
}
fn parse<T: serde::de::DeserializeOwned>(
    p: Option<&RawValue>,
    missing: &str,
    bad: &str,
) -> Result<T, RpcError> {
    let p = p.ok_or_else(|| invalid(missing))?;
    serde_json::from_str(p.get()).map_err(|_| invalid(bad))
}
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    if !matches!(
        method,
        "instance.create" | "instance.list" | "instance.get" | "instance.delete"
    ) {
        return None;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = params;
        Some(Err(fail(
            "NOT_IMPLEMENTED",
            "Rust rewrite instance filesystem adapter is not implemented on this platform",
        )))
    }
    #[cfg(any(unix, windows))]
    {
        Some(handle_unix(method, params))
    }
}
#[cfg(any(unix, windows))]
fn handle_unix(method: &str, p: Option<&RawValue>) -> Result<Value, RpcError> {
    let open = |path: &str| {
        Root::open(path)
            .map_err(|_| fail("INSTANCES_ROOT_ERROR", "Failed to open the instances root"))
    };
    match method {
        "instance.create" => {
            let c: Create = parse(
                p,
                "instance creation requires directory, id, and version_id",
                "instance creation parameters are invalid",
            )?;
            if c.directory.is_empty() || c.id.is_empty() || c.version_id.is_empty() {
                return Err(invalid("instance creation parameters are out of range"));
            }
            validate_id(&c.id).map_err(|_| invalid("the instance id is invalid"))?;
            if !matches!(c.source.as_str(), "official" | "bmclapi") {
                return Err(invalid("source must be official or bmclapi"));
            }
            if [&c.fabric_loader, &c.neoforge_version, &c.forge_version]
                .iter()
                .filter(|s| s.is_some())
                .count()
                > 1
            {
                return Err(invalid("only one loader field may be set"));
            }
            if c.validate {
                validate_selection(
                    &c.version_id,
                    c.fabric_loader.as_deref(),
                    c.neoforge_version.as_deref(),
                    c.forge_version.as_deref(),
                    &c.source,
                )
                .map_err(resolution_error)?;
            }
            let root = open(&c.directory)?;
            let m = Manifest {
                schema_version: 1,
                name: c.name.unwrap_or_else(|| c.id.clone()),
                id: c.id,
                version_id: c.version_id,
                fabric_loader: c.fabric_loader,
                neoforge_version: c.neoforge_version,
                forge_version: c.forge_version,
                source: c.source,
            };
            root.create(&m).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    fail("INSTANCE_EXISTS", "An instance with this id already exists")
                } else {
                    fail("INSTANCE_CREATE_FAILED", "Failed to create the instance")
                }
            })?;
            let mut result = json!({"stage":"instance.create"});
            result
                .as_object_mut()
                .unwrap()
                .extend(m.fields(false).as_object().unwrap().clone());
            Ok(result)
        }
        "instance.list" => {
            let d: Directory = parse(
                p,
                "instance listing requires a directory",
                "instance listing parameters are invalid",
            )?;
            if d.directory.is_empty() {
                return Err(invalid("instance listing directory must not be empty"));
            }
            let root = open(&d.directory)?;
            let manifests = root
                .list()
                .map_err(|_| fail("INSTANCE_LIST_FAILED", "Failed to list instances"))?;
            let items: Result<Vec<Value>, RpcError> = manifests
                .iter()
                .map(|m| {
                    root.installed(m).map(|v| m.fields(v)).map_err(|_| {
                        fail(
                            "INSTANCE_STATUS_FAILED",
                            "Failed to read instance installation status",
                        )
                    })
                })
                .collect();
            Ok(json!({"stage":"instance.list","instances":items?}))
        }
        _ => {
            let is_get = method == "instance.get";
            let t: Target = if is_get {
                parse(
                    p,
                    "instance lookup requires a directory and an id",
                    "instance lookup parameters are invalid",
                )?
            } else {
                parse(
                    p,
                    "instance deletion requires a directory and an id",
                    "instance deletion parameters are invalid",
                )?
            };
            if t.directory.is_empty() || t.id.is_empty() {
                return Err(invalid(if is_get {
                    "instance lookup parameters are out of range"
                } else {
                    "instance deletion parameters are out of range"
                }));
            }
            let root = open(&t.directory)?;
            let m = root.load(&t.id).map_err(|e| {
                if validate_id(&t.id).is_err() || e.kind() == std::io::ErrorKind::NotFound {
                    fail("INSTANCE_NOT_FOUND", "No instance matches the given id")
                } else if is_get {
                    fail("INSTANCE_LOAD_FAILED", "Failed to load the instance")
                } else {
                    fail("INSTANCE_DELETE_FAILED", "Failed to delete the instance")
                }
            })?;
            if is_get {
                let status = root.installed(&m).map_err(|_| {
                    fail(
                        "INSTANCE_STATUS_FAILED",
                        "Failed to read instance installation status",
                    )
                })?;
                let mut result = json!({"stage":"instance.get"});
                result
                    .as_object_mut()
                    .unwrap()
                    .extend(m.fields(status).as_object().unwrap().clone());
                Ok(result)
            } else {
                root.delete(&t.id)
                    .map_err(|_| fail("INSTANCE_DELETE_FAILED", "Failed to delete the instance"))?;
                Ok(json!({"stage":"instance.delete","id":t.id,"deleted":true}))
            }
        }
    }
}
#[derive(Deserialize)]
struct Installation {
    #[serde(default = "one", deserialize_with = "schema_number")]
    schema_version: u32,
    version_id: String,
    #[serde(default)]
    fabric_loader: Option<String>,
    #[serde(default)]
    neoforge_version: Option<String>,
    #[serde(default)]
    forge_version: Option<String>,
    #[serde(default = "official")]
    source: String,
    os: String,
    arch: String,
}
fn installed_bytes(bytes: &[u8], m: &Manifest) -> bool {
    if bytes.len() > 16 * 1024 {
        return false;
    }
    let Ok(i) = serde_json::from_slice::<Installation>(bytes) else {
        return false;
    };
    if i.schema_version != 1
        || i.version_id.is_empty()
        || i.version_id.len() > 256
        || i.os.is_empty()
        || i.os.len() > 32
        || i.arch.is_empty()
        || i.arch.len() > 32
        || !matches!(i.source.as_str(), "official" | "bmclapi")
    {
        return false;
    }
    let loaders = [&i.fabric_loader, &i.neoforge_version, &i.forge_version];
    if loaders.iter().filter(|x| x.is_some()).count() > 1
        || loaders
            .iter()
            .filter_map(|v| v.as_ref())
            .any(|x| x.is_empty() || x.len() > 256)
    {
        return false;
    }
    i.version_id == m.version_id
        && i.fabric_loader == m.fabric_loader
        && i.neoforge_version == m.neoforge_version
        && i.forge_version == m.forge_version
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_ids() {
        for id in ["a", "fabric_test", "vanilla-26.1"] {
            validate_id(id).unwrap();
        }
        for id in [
            "",
            ".",
            "..",
            "Upper",
            "has space",
            "../escape",
            "con",
            "com1",
            "name.",
        ] {
            assert!(validate_id(id).is_err());
        }
    }
    #[test]
    fn manifest_defaults_and_unknown_fields() {
        let m = Manifest::parse(
            br#"{"id":"a","name":"A","version_id":"1.21","extra":true}"#,
            "a",
        )
        .unwrap();
        assert_eq!(m.source, "official");
        assert_eq!(m.schema_version, 1);
        assert_eq!(Manifest::parse(&m.encode().unwrap(), "a").unwrap(), m);
    }
    #[test]
    fn installed_source_intentionally_not_compared() {
        let m = Manifest::parse(br#"{"id":"a","name":"A","version_id":"1.21"}"#, "a").unwrap();
        assert!(installed_bytes(
            br#"{"version_id":"1.21","source":"bmclapi","os":"linux","arch":"x86_64"}"#,
            &m
        ));
    }
}

#[cfg(test)]
mod numeric_schema_edges {
    use super::*;
    #[test]
    fn disk_schema_uses_binary128_but_dynamic_params_keep_binary64() {
        for number in ["1.0000000000000001", "\"1.0000000000000001\""] {
            let raw = format!(
                r#"{{"schema_version":{number},"id":"demo","name":"Demo","version_id":"1.0"}}"#
            );
            assert_eq!(
                parse_document::<Manifest>(raw.as_bytes()).unwrap_err(),
                "InvalidNumber"
            );
            assert_eq!(
                crate::servers::manifest::Manifest::parse(raw.as_bytes(), "demo").unwrap_err(),
                "InvalidNumber"
            );
        }
        #[derive(Deserialize)]
        struct Dynamic {
            #[serde(deserialize_with = "dynamic_schema_number")]
            number: u32,
        }
        let raw = r#"{"number":1.0000000000000001}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(serde_json::from_value::<Dynamic>(value).unwrap().number, 1);
        let raw = r#"{"number":"1.0000000000000001"}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        assert!(serde_json::from_value::<Dynamic>(value).is_err());
    }
    #[test]
    fn disk_numbers_and_ignored_fields_retain_their_lexemes() {
        let raw = br#"{"schema_version":1e9999,"id":"demo","name":"Demo","version_id":"1.0"}"#;
        assert_eq!(parse_document::<Manifest>(raw).unwrap_err(), "Overflow");
        let raw =
            br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"1.0","unused":1e9999}"#;
        assert_eq!(parse_document::<Manifest>(raw).unwrap().schema_version, 1);
    }
}

#[cfg(test)]
mod validation_errors {
    use super::*;
    #[test]
    fn resolution_timeout_remains_distinct() {
        let timeout = resolution_error(crate::metadata::Error::MetadataFetchTimeout);
        assert_eq!(timeout.code, "METADATA_TIMEOUT");
        assert_eq!(
            timeout.message,
            "Timed out while resolving the Minecraft version and loader"
        );
        let missing = resolution_error(crate::metadata::Error::VersionNotFound);
        assert_eq!(missing.code, "VERSION_RESOLUTION_FAILED");
        assert_eq!(
            missing.message,
            "Failed to resolve the Minecraft version and loader"
        );
    }
}

//! Pinned instance schema and offline JSONL operations. Network validation remains explicit.
use crate::protocol::RpcError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
mod byte_read;
mod cli;
#[cfg(any(unix, windows))]
mod create_unvalidated;
mod manifest_ordered;
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
// DownloadSource is a typed enum in Zig: invalid tags fail while parsing,
// before post-parse manifest validation, even when another field is invalid.
// Keep the public String representation and callers' construction API unchanged.
pub(crate) fn schema_source<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    let token = raw.get().trim();
    let value = if token.starts_with('"') {
        serde_json::from_str::<String>(token).map_err(serde::de::Error::custom)?
    } else if token.starts_with('-') || token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        token.to_owned()
    } else {
        return Err(serde::de::Error::custom("Zig:UnexpectedToken"));
    };
    source_tag(&value)
        .map(str::to_owned)
        .ok_or_else(|| serde::de::Error::custom("Zig:InvalidEnumTag"))
}
fn source_tag(value: &str) -> Option<&'static str> {
    match value {
        "official" => return Some("official"),
        "bmclapi" => return Some("bmclapi"),
        _ => {}
    }
    // Shared with the ordered persisted schema; the public serde path is unchanged.
    if value != "-0"
        && !value.bytes().any(|b| matches!(b, b'.' | b'e' | b'E'))
        && let Ok(tag) = crate::json_number::unsigned_text(value, 1)
    {
        return Some(if tag == 0 { "official" } else { "bmclapi" });
    }
    None
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
#[cfg(any(test, target_os = "linux"))]
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
pub(crate) fn parse_document<T: manifest_ordered::Document>(
    bytes: &[u8],
) -> Result<T, &'static str> {
    manifest_ordered::parse(bytes)
}
// Display names are byte slices in the persisted Zig schemas. Only read views
// use this representation; operational Manifest construction remains String-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayName(pub(crate) Vec<u8>);
impl<'de> Deserialize<'de> for DisplayName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <&RawValue>::deserialize(d)?;
        if raw.get().starts_with('"') {
            return serde_json::from_str::<String>(raw.get())
                .map(|s| Self(s.into_bytes()))
                .map_err(serde::de::Error::custom);
        }
        let elements: Vec<&RawValue> =
            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
        elements
            .into_iter()
            .map(|v| {
                crate::json_number::typed_raw(v.get(), 8)
                    .map(|n| n as u8)
                    .map_err(|e| serde::de::Error::custom(format!("Zig:{}", e.name())))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}
impl Serialize for DisplayName {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => s.serialize_str(text),
            Err(_) => self.0.serialize(s),
        }
    }
}
// Installation identity deliberately excludes display names. Borrowed fields
// prevent either a placeholder name or lossy conversion entering operational APIs.
#[derive(Clone, Copy)]
pub(crate) struct InstallationIdentity<'a> {
    pub id: &'a str,
    pub version_id: &'a str,
    pub fabric_loader: &'a Option<String>,
    pub neoforge_version: &'a Option<String>,
    pub forge_version: &'a Option<String>,
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
    #[serde(default = "official", deserialize_with = "schema_source")]
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReadManifest {
    #[serde(default = "one", deserialize_with = "schema_number")]
    pub schema_version: u32,
    pub id: String,
    pub name: crate::instances::DisplayName,
    pub version_id: String,
    #[serde(default)]
    pub fabric_loader: Option<String>,
    #[serde(default)]
    pub neoforge_version: Option<String>,
    #[serde(default)]
    pub forge_version: Option<String>,
    #[serde(default = "official", deserialize_with = "schema_source")]
    pub source: String,
}
impl Manifest {
    #[cfg(test)]
    pub(crate) fn identity(&self) -> crate::instances::InstallationIdentity<'_> {
        crate::instances::InstallationIdentity {
            id: &self.id,
            version_id: &self.version_id,
            fabric_loader: &self.fabric_loader,
            neoforge_version: &self.neoforge_version,
            forge_version: &self.forge_version,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        Validation {
            id: &self.id,
            schema_version: self.schema_version,
            name: self.name.as_bytes(),
            version_id: &self.version_id,
            fabric_loader: &self.fabric_loader,
            neoforge_version: &self.neoforge_version,
            forge_version: &self.forge_version,
            source: &self.source,
        }
        .validate()
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
    directory: LookupDirectory,
}
// Lookup and delete parameters use byte IDs; creation remains unchanged.
// Keep invalid bytes until range and root-open checks.
#[derive(Debug)]
pub(crate) struct LookupId(Vec<u8>);
impl<'de> Deserialize<'de> for LookupId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <&RawValue>::deserialize(d)?;
        if raw.get().starts_with('"') {
            return serde_json::from_str::<String>(raw.get())
                .map(|s| Self(s.into_bytes()))
                .map_err(serde::de::Error::custom);
        }
        let elements: Vec<&RawValue> =
            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
        elements
            .into_iter()
            .map(|v| {
                crate::json_number::dynamic_raw(v.get(), 8)
                    .map(|n| n as u8)
                    .map_err(|e| serde::de::Error::custom(e.name()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}
impl LookupId {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn validated(&self) -> Option<&str> {
        let id = std::str::from_utf8(&self.0).ok()?;
        validate_id(id).ok()?;
        Some(id)
    }
}
// Directory byte decoding is separate from validated IDs. Get/list/delete use
// this view; native conversion uses the existing checked platform bridge.
#[derive(Debug)]
pub(crate) struct LookupDirectory(Vec<u8>);
impl<'de> Deserialize<'de> for LookupDirectory {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        LookupId::deserialize(d).map(|bytes| Self(bytes.0))
    }
}
impl LookupDirectory {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn native(&self) -> std::io::Result<std::ffi::OsString> {
        crate::store::path_from_bytes(&self.0)
    }
}
#[derive(Deserialize)]
pub(crate) struct LookupTarget {
    pub directory: LookupDirectory,
    pub id: LookupId,
}
pub(crate) fn lookup_target(
    p: Option<&RawValue>,
    missing: &str,
    bad: &str,
) -> Result<LookupTarget, RpcError> {
    let p = p.ok_or_else(|| invalid(missing))?;
    if !p.get().trim_start().starts_with('{') {
        return Err(invalid(bad));
    }
    serde_json::from_str(p.get()).map_err(|_| invalid(bad))
}
// Create retains String operational consumers. Accept source-compatible UTF-8
// byte arrays without widening those consumers to opaque byte strings here.
fn create_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let bytes = LookupId::deserialize(d)?;
    String::from_utf8(bytes.0).map_err(serde::de::Error::custom)
}
fn create_optional_text<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    Option::<LookupId>::deserialize(d)?
        .map(|bytes| String::from_utf8(bytes.0).map_err(serde::de::Error::custom))
        .transpose()
}
#[derive(Deserialize)]
struct Create {
    #[serde(deserialize_with = "create_text")]
    directory: String,
    #[serde(deserialize_with = "create_text")]
    id: String,
    #[serde(deserialize_with = "create_text")]
    version_id: String,
    #[serde(default, deserialize_with = "create_optional_text")]
    name: Option<String>,
    #[serde(default, deserialize_with = "create_optional_text")]
    fabric_loader: Option<String>,
    #[serde(default, deserialize_with = "create_optional_text")]
    neoforge_version: Option<String>,
    #[serde(default, deserialize_with = "create_optional_text")]
    forge_version: Option<String>,
    #[serde(default = "official", deserialize_with = "create_text")]
    source: String,
    #[serde(default = "yes")]
    validate: bool,
}
fn parse_create(p: Option<&RawValue>) -> Result<Create, RpcError> {
    if p.is_some_and(|raw| !raw.get().trim_start().starts_with('{')) {
        return Err(invalid("instance creation parameters are invalid"));
    }
    parse(
        p,
        "instance creation requires directory, id, and version_id",
        "instance creation parameters are invalid",
    )
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
            if create_unvalidated::selected(p) {
                return create_unvalidated::prepare(p)?.execute();
            }
            let c = parse_create(p)?;
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
        "instance.get" => {
            let t = lookup_target(
                p,
                "instance lookup requires a directory and an id",
                "instance lookup parameters are invalid",
            )?;
            if t.directory.is_empty() || t.id.is_empty() {
                return Err(invalid("instance lookup parameters are out of range"));
            }
            let root = t
                .directory
                .native()
                .and_then(|path| Root::open_native(&path))
                .map_err(|_| fail("INSTANCES_ROOT_ERROR", "Failed to open the instances root"))?;
            let id = t
                .id
                .validated()
                .ok_or_else(|| fail("INSTANCE_NOT_FOUND", "No instance matches the given id"))?;
            let m = root.load_read(id).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound || e.to_string() == "InvalidInstanceId"
                {
                    fail("INSTANCE_NOT_FOUND", "No instance matches the given id")
                } else {
                    fail("INSTANCE_LOAD_FAILED", "Failed to load the instance")
                }
            })?;
            let status = root.installed_identity(m.identity()).map_err(|_| {
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
        }
        "instance.list" => {
            if p.is_some_and(|raw| !raw.get().trim_start().starts_with('{')) {
                return Err(invalid("instance listing parameters are invalid"));
            }
            let d: Directory = parse(
                p,
                "instance listing requires a directory",
                "instance listing parameters are invalid",
            )?;
            if d.directory.is_empty() {
                return Err(invalid("instance listing directory must not be empty"));
            }
            let root = d
                .directory
                .native()
                .and_then(|path| Root::open_native(&path))
                .map_err(|_| fail("INSTANCES_ROOT_ERROR", "Failed to open the instances root"))?;
            let manifests = root
                .list_read()
                .map_err(|_| fail("INSTANCE_LIST_FAILED", "Failed to list instances"))?;
            let items: Result<Vec<Value>, RpcError> = manifests
                .iter()
                .map(|m| {
                    root.installed_identity(m.identity())
                        .map(|v| m.fields(v))
                        .map_err(|_| {
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
            let t = lookup_target(
                p,
                "instance deletion requires a directory and an id",
                "instance deletion parameters are invalid",
            )?;
            if t.directory.is_empty() || t.id.is_empty() {
                return Err(invalid("instance deletion parameters are out of range"));
            }
            let root = t
                .directory
                .native()
                .and_then(|path| Root::open_native(&path))
                .map_err(|_| fail("INSTANCES_ROOT_ERROR", "Failed to open the instances root"))?;
            let id = t
                .id
                .validated()
                .ok_or_else(|| fail("INSTANCE_NOT_FOUND", "No instance matches the given id"))?;
            let delete_error = |e: std::io::Error| {
                if e.kind() == std::io::ErrorKind::NotFound || e.to_string() == "InvalidInstanceId"
                {
                    fail("INSTANCE_NOT_FOUND", "No instance matches the given id")
                } else {
                    fail("INSTANCE_DELETE_FAILED", "Failed to delete the instance")
                }
            };
            root.load_read(id).map_err(delete_error)?;
            root.delete(id).map_err(delete_error)?;
            Ok(json!({"stage":"instance.delete","id":id,"deleted":true}))
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
fn installed_bytes(bytes: &[u8], m: InstallationIdentity<'_>) -> bool {
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
        && &i.fabric_loader == m.fabric_loader
        && &i.neoforge_version == m.neoforge_version
        && &i.forge_version == m.forge_version
}
struct Validation<'a> {
    id: &'a str,
    schema_version: u32,
    name: &'a [u8],
    version_id: &'a str,
    fabric_loader: &'a Option<String>,
    neoforge_version: &'a Option<String>,
    forge_version: &'a Option<String>,
    source: &'a str,
}
impl Validation<'_> {
    fn validate(&self) -> Result<(), &'static str> {
        validate_id(self.id)?;
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
        if !matches!(self.source, "official" | "bmclapi") {
            return Err("InvalidEnumTag");
        }
        Ok(())
    }
}

impl ReadManifest {
    pub(crate) fn identity(&self) -> crate::instances::InstallationIdentity<'_> {
        crate::instances::InstallationIdentity {
            id: &self.id,
            version_id: &self.version_id,
            fabric_loader: &self.fabric_loader,
            neoforge_version: &self.neoforge_version,
            forge_version: &self.forge_version,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        Validation {
            id: &self.id,
            schema_version: self.schema_version,
            name: &self.name.0,
            version_id: &self.version_id,
            fabric_loader: &self.fabric_loader,
            neoforge_version: &self.neoforge_version,
            forge_version: &self.forge_version,
            source: &self.source,
        }
        .validate()
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
    pub fn fields(&self, installed: bool) -> Value {
        json!({"id":self.id,"name":self.name,"version_id":self.version_id,"fabric_loader":self.fabric_loader,"neoforge_version":self.neoforge_version,"forge_version":self.forge_version,"source":self.source,"installed":installed})
    }
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
            m.identity()
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

#[cfg(test)]
mod manifest_enum_tests {
    fn instance(bytes: &[u8]) -> Result<String, &'static str> {
        super::Manifest::parse(bytes, "demo").map(|m| m.source)
    }
    fn server(bytes: &[u8]) -> Result<String, &'static str> {
        crate::servers::manifest::Manifest::parse(bytes, "demo").map(|m| m.source)
    }
    #[test]
    fn source_enum_tags_ordinals_and_precedence() {
        for parse in [instance, server] {
            for (token, expected) in [
                (r#""official""#, "official"),
                (r#""bmclapi""#, "bmclapi"),
                ("0", "official"),
                ("1", "bmclapi"),
                (r#""+1""#, "bmclapi"),
                (r#""0__1""#, "bmclapi"),
                (r#""-00""#, "official"),
            ] {
                let input = format!(
                    r#"{{"id":"demo","name":"Demo","version_id":"fake","source":{token}}}"#
                );
                assert_eq!(parse(input.as_bytes()).unwrap(), expected);
            }
            for token in [
                r#""bad""#, "2", "-1", "-0", r#""-0""#, "1.0", "1e0", r#""1.0""#, r#""1_""#,
            ] {
                let input = format!(r#"{{"name":"","source":{token}}}"#);
                assert_eq!(
                    parse(input.as_bytes()).unwrap_err(),
                    "InvalidEnumTag",
                    "{token}"
                );
            }
            assert_eq!(
                parse(br#"{"name":"","source":[]}"#).unwrap_err(),
                "UnexpectedToken"
            );
            assert_eq!(
                parse(br#"{"name":"A","name":"B","source":"bad"}"#).unwrap_err(),
                "DuplicateField"
            );
            assert_eq!(
                parse(br#"{"source":"bad","schema_version":4294967296}"#).unwrap_err(),
                "InvalidEnumTag"
            );
            assert_eq!(
                parse(br#"{"schema_version":4294967296,"source":"bad"}"#).unwrap_err(),
                "Overflow"
            );
        }
    }
}

#[cfg(test)]
mod display_name_tests {
    use super::*;

    #[test]
    fn read_view_preserves_bytes_but_operational_manifest_stays_string_based() {
        for (token, expected) in [
            (r#""Demo""#, b"Demo".as_slice()),
            ("[68,101,109,111]", b"Demo".as_slice()),
            ("[255,9,10,0,65]", b"\xff\t\n\0A".as_slice()),
            (r#"["+6__5",66.0,6.7e1]"#, b"ABC".as_slice()),
        ] {
            let bytes = format!(r#"{{"id":"a","name":{token},"version_id":"fake"}}"#);
            let m = ReadManifest::parse(bytes.as_bytes(), "a").unwrap();
            assert_eq!(m.name.0, expected);
            let encoded = serde_json::to_value(&m.name).unwrap();
            if let Ok(text) = std::str::from_utf8(expected) {
                assert_eq!(encoded, json!(text));
            } else {
                assert_eq!(encoded, json!(expected));
            }
            assert_eq!(
                Manifest::parse(bytes.as_bytes(), "a").is_ok(),
                token.starts_with('"')
            );
            assert!(installed_bytes(
                br#"{"version_id":"fake","source":"bmclapi","os":"linux","arch":"x86_64"}"#,
                m.identity()
            ));
            assert!(!installed_bytes(
                br#"{"version_id":"other","source":"official","os":"linux","arch":"x86_64"}"#,
                m.identity()
            ));
        }
    }

    #[test]
    fn read_view_ordered_validation_matches_operational_string_validation() {
        for (name, expected) in [
            ("[]", "InvalidInstanceName"),
            ("[256]", "Overflow"),
            ("[1.0000000000000001]", "InvalidNumber"),
            ("[null]", "UnexpectedToken"),
        ] {
            let bytes = format!(r#"{{"id":"a","name":{name},"version_id":""}}"#);
            assert_eq!(
                ReadManifest::parse(bytes.as_bytes(), "a").unwrap_err(),
                expected
            );
        }
        for document in [
            r#"{"id":"a","name":"","version_id":""}"#,
            r#"{"id":"../bad","name":"","version_id":""}"#,
            r#"{"schema_version":2,"id":"a","name":"","version_id":""}"#,
            r#"{"id":"a","name":"","version_id":"","source":"bad"}"#,
            r#"{"id":"a","name":"A","version_id":"x","fabric_loader":"a","forge_version":"b"}"#,
        ] {
            assert_eq!(
                ReadManifest::parse(document.as_bytes(), "a").unwrap_err(),
                Manifest::parse(document.as_bytes(), "a").unwrap_err()
            );
        }
    }
}

#[cfg(test)]
mod lookup_id_tests {
    use super::*;
    #[test]
    fn dynamic_lookup_bytes_do_not_use_persisted_typed_precision() {
        let raw = "[100.000000000000001,101,109,111]";
        assert_eq!(
            serde_json::from_str::<LookupId>(raw).unwrap().validated(),
            Some("demo")
        );
        assert!(serde_json::from_str::<DisplayName>(raw).is_err());
        assert!(
            serde_json::from_str::<LookupId>(r#"["100.000000000000001",101,109,111]"#).is_err()
        );
        for raw in ["[255]", "[0]", "[195,169]", "[46,46,47,120]"] {
            let id = serde_json::from_str::<LookupId>(raw).unwrap();
            assert!(!id.is_empty());
            assert_eq!(id.validated(), None);
        }
        assert!(serde_json::from_str::<LookupId>("[]").unwrap().is_empty());
        for raw in ["[256]", "[-1]", "[true]", "[null]", "[[100]]"] {
            assert!(serde_json::from_str::<LookupId>(raw).is_err());
        }
    }
    #[test]
    fn lookup_params_ignore_unrelated_fields_and_create_accepts_utf8_arrays() {
        let bytes = r#"{"directory":"instances","id":[100,101,109,111],"unknown":1e400,"java_paths":false}"#;
        let raw = RawValue::from_string(bytes.into()).unwrap();
        let p = lookup_target(Some(&raw), "missing", "invalid").unwrap();
        assert_eq!(p.id.validated(), Some("demo"));
        let create = r#"{"directory":"instances","id":[100,101,109,111],"version_id":"fake"}"#;
        assert!(serde_json::from_str::<Create>(create).is_ok());
        let raw = RawValue::from_string(r#"["instances","demo"]"#.into()).unwrap();
        assert!(lookup_target(Some(&raw), "missing", "invalid").is_err());
    }
}

#[cfg(test)]
mod lookup_directory_tests {
    use super::*;
    #[test]
    fn directory_bytes_use_checked_native_encoding_without_id_validation() {
        let d: LookupDirectory = serde_json::from_str(r#""目录/child""#).unwrap();
        assert_eq!(d.native().unwrap(), std::ffi::OsString::from("目录/child"));
        let raw: LookupDirectory = serde_json::from_str("[255]").unwrap();
        let surrogate: LookupDirectory = serde_json::from_str("[237,160,128]").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(raw.native().unwrap().as_bytes(), &[255]);
            assert_eq!(surrogate.native().unwrap().as_bytes(), &[237, 160, 128]);
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            assert_eq!(raw.native().unwrap_err().to_string(), "BadPathName");
            assert_eq!(
                surrogate
                    .native()
                    .unwrap()
                    .encode_wide()
                    .collect::<Vec<_>>(),
                vec![0xd800]
            );
        }
        let empty: LookupDirectory = serde_json::from_str("[]").unwrap();
        assert!(empty.is_empty());
        assert!(serde_json::from_str::<LookupDirectory>("[256]").is_err());
    }
}

#[cfg(test)]
mod create_params_tests {
    use super::*;
    use std::process::Command;

    // Parse only: never call handle, validate_selection, Root, or a create method.
    // Lexical/duplicate controls characterize this helper boundary, not whether
    // outer protocol guards permit the same input to reach an actual RPC.
    fn normalized(raw: &str) -> Value {
        let raw = match RawValue::from_string(raw.to_owned()) {
            Ok(raw) => raw,
            Err(_) => return json!({"invalid":true}),
        };
        match parse_create(Some(&raw)) {
            Err(_) => json!({"invalid":true}),
            Ok(c) => json!({"value": {
                "directory":c.directory,"id":c.id,"name":c.name,"version_id":c.version_id,
                "fabric_loader":c.fabric_loader,"neoforge_version":c.neoforge_version,
                "forge_version":c.forge_version,"source":c.source,"validate":c.validate
            }}),
        }
    }

    #[test]
    fn create_parameter_defaults_and_optional_nulls() {
        let raw = RawValue::from_string(r#"{"directory":"unused","id":"demo","version_id":"1.20","name":null,"fabric_loader":null,"neoforge_version":null,"forge_version":null}"#.into()).unwrap();
        let c = parse_create(Some(&raw)).unwrap();
        assert_eq!(c.source, "official");
        assert!(
            c.validate
                && c.name.is_none()
                && c.fabric_loader.is_none()
                && c.neoforge_version.is_none()
                && c.forge_version.is_none()
        );
        for field in ["source", "validate", "directory", "id", "version_id"] {
            let mut value: Value = serde_json::from_str(raw.get()).unwrap();
            value[field] = Value::Null;
            let invalid = RawValue::from_string(value.to_string()).unwrap();
            assert!(
                parse_create(Some(&invalid)).is_err(),
                "{field} must not accept null"
            );
        }
        assert!(parse_create(None).is_err());
    }

    #[test]
    fn explicit_false_keeps_opaque_create_decode_gap_visible() {
        let base = serde_json::json!({"directory":"unused", "id":"demo", "version_id":"v", "validate":false});
        let raw = RawValue::from_string(base.to_string()).unwrap();
        assert!(!parse_create(Some(&raw)).unwrap().validate);
        // Observation of the recovered decoder, not source-parity acceptance.
        // Do not invoke the create handler, resolver or writer here.
        for field in [
            "directory",
            "id",
            "version_id",
            "name",
            "fabric_loader",
            "neoforge_version",
            "forge_version",
            "source",
        ] {
            let mut value = base.clone();
            value[field] = serde_json::json!([255]);
            let raw = RawValue::from_string(value.to_string()).unwrap();
            let error = parse_create(Some(&raw))
                .err()
                .expect("opaque bytes currently rejected during decoding");
            assert_eq!(
                error,
                invalid("instance creation parameters are invalid"),
                "{field}"
            );
        }
    }

    #[test]
    #[ignore = "pure source-pinned UTF-8 parameter slice; nine residual rows remain explicit"]
    fn pinned_create_utf8_parameter_schema_only() {
        run_schema();
    }
    fn run_schema() {
        let source =
            std::path::PathBuf::from(std::env::var_os("JMCL_ZIG_SOURCE").expect("JMCL_ZIG_SOURCE"));
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&source)
            .output()
            .unwrap();
        assert!(head.status.success());
        assert_eq!(
            String::from_utf8(head.stdout).unwrap().trim(),
            "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
        );
        let clean = Command::new("git")
            .args([
                "diff",
                "--exit-code",
                "HEAD",
                "--",
                "src/protocol/instance.zig",
            ])
            .current_dir(&source)
            .output()
            .unwrap();
        assert!(
            clean.status.success(),
            "pinned parameter source has local changes"
        );
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/instance_create_params.zig");
        let adapter = std::fs::read_to_string(&fixture)
            .unwrap()
            .replace("\r\n", "\n");
        let original = std::fs::read_to_string(source.join("src/protocol/instance.zig"))
            .unwrap()
            .replace("\r\n", "\n");
        let declaration = |text: &str| {
            let begin = text.find("const InstanceCreateParams = struct {").unwrap();
            let end = begin + text[begin..].find("\n};").unwrap() + 3;
            text[begin..end].to_owned()
        };
        assert_eq!(declaration(&adapter), declaration(&original));
        // Adapter has no Core import or handler; only std JSON parsing + owned fixture IO.
        assert!(!adapter.contains("jmcl_core") && !adapter.contains("writeCreateResult"));
        let zig = std::env::var_os("JMCL_ZIG_COMPILER").unwrap_or_else(|| "zig".into());
        let version = Command::new(&zig).arg("version").output().unwrap();
        assert!(version.status.success());
        assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "0.16.0");
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join(if cfg!(windows) {
            "params.exe"
        } else {
            "params"
        });
        let build = Command::new(&zig)
            .arg("build-exe")
            .arg(&fixture)
            .arg("-OReleaseSafe")
            .arg(format!("-femit-bin={}", executable.display()))
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let base = json!({"directory":"owned-unused-root","id":"demo","version_id":"1.20"});
        let mut cases = vec![("defaults".to_owned(), base.to_string(), true)];
        let fields = [
            "directory",
            "id",
            "name",
            "version_id",
            "fabric_loader",
            "neoforge_version",
            "forge_version",
            "source",
        ];
        let mut all = base.clone();
        for field in fields {
            all[field] = json!([65, 83, 67, 73, 73]);
        }
        cases.push(("all-eight-ascii-arrays".into(), all.to_string(), true));
        for field in fields {
            for (label, bytes) in [
                ("unicode", vec![195, 169]),
                ("opaque", vec![255]),
                ("empty", vec![]),
            ] {
                let mut v = base.clone();
                v[field] = json!(bytes);
                cases.push((format!("{field}-{label}-array"), v.to_string(), true));
            }
        }
        for field in ["directory", "id", "version_id"] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            cases.push((format!("{field}-missing"), missing.to_string(), false));
            let mut null = base.clone();
            null[field] = Value::Null;
            cases.push((format!("{field}-null"), null.to_string(), false));
        }
        for (label, field, value, accepted) in [
            ("optional-null", "name", Value::Null, true),
            ("validate-false", "validate", json!(false), true),
            ("validate-number", "validate", json!(0), false),
            ("source-ordinal-not-enum", "source", json!(0), false),
            (
                "unknown-source-is-parse-valid",
                "source",
                json!("not-a-source"),
                true,
            ),
            ("overflow-byte", "name", json!([256]), false),
            ("negative-byte", "name", json!([-1]), false),
            ("bool-byte", "name", json!([true]), false),
            ("null-byte", "name", json!([null]), false),
            ("nested-byte", "name", json!([[65]]), false),
        ] {
            let mut v = base.clone();
            v[field] = value;
            cases.push((label.into(), v.to_string(), accepted));
        }
        for (label, value, accepted) in [
            ("raw-rounded-byte", "[100.0000000000000001]", true),
            ("quoted-precise-byte", "[\"100.0000000000000001\"]", false),
            ("quoted-underscore-byte", "[\"1_0_0\"]", true),
        ] {
            cases.push((
                label.into(),
                format!(
                    "{{\"directory\":\"owned-unused-root\",\"id\":{value},\"version_id\":\"1.20\"}}"
                ),
                accepted,
            ));
        }
        for (label, raw) in [
            ("null-object", "null"),
            ("scalar-object", "42"),
            (
                "positional-sequence",
                r#"["owned-unused-root","demo","1.20",null,null,null,null,"official",false]"#,
            ),
        ] {
            cases.push((label.into(), raw.into(), false));
        }
        let mut unknown = base.to_string();
        unknown.pop();
        unknown.push_str(",\"unused\":1e400}");
        cases.push(("unknown-large-number".into(), unknown, true));
        for (label, tail, accepted) in [
            ("duplicate-known-field", ",\"id\":\"last\"}", false),
            ("unknown-lone-surrogate", ",\"unused\":\"\\ud800\"}", false),
            (
                "unknown-paired-surrogate",
                ",\"unused\":\"\\ud83d\\ude00\"}",
                true,
            ),
            ("unknown-malformed-token", ",\"unused\":!}", false),
        ] {
            let mut raw = base.to_string();
            raw.pop();
            raw.push_str(tail);
            cases.push((label.into(), raw, accepted));
        }
        let mut differences = vec![];
        for (label, raw, accepted) in &cases {
            let input = temp.path().join("input.json");
            std::fs::write(&input, raw).unwrap();
            let result = Command::new(&executable).arg(&input).output().unwrap();
            assert!(
                result.status.success() && result.stderr.is_empty(),
                "{label}"
            );
            let expected: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(
                expected.get("value").is_some(),
                *accepted,
                "source acceptance {label}: {expected}"
            );
            if label == "defaults" {
                assert_eq!(expected["value"]["source"], "official");
                assert_eq!(expected["value"]["validate"], true);
            }
            let actual = normalized(raw);
            eprintln!(
                "CREATE_PARAMS_ROW {}",
                json!({"case":label,"input":raw,"source":expected,"rust":actual})
            );
            if label.ends_with("-opaque-array") {
                // These source-accepted opaque fields remain unsupported by
                // the existing operational String consumer, not parity.
                let field = label.strip_suffix("-opaque-array").unwrap();
                assert!(fields.contains(&field));
                assert_eq!(expected["value"][field], json!([255]));
                assert_eq!(actual, json!({"invalid":true}));
            } else if label == "unknown-lone-surrogate" {
                // The outer RPC already rejects this; helper boundary only.
                assert_eq!(expected, json!({"invalid":true}));
                assert_eq!(actual, normalized(&base.to_string()));
            } else {
                assert_eq!(actual, expected, "strict UTF-8/object subset {label}");
            }
            if actual != expected {
                differences.push(label.clone());
                eprintln!("CREATE_PARAMS_DIFFERENCE {label}: source={expected} rust={actual}");
            }
        }
        eprintln!(
            "pure instance.create parameter schema: {} cases; {} differences; no handler calls",
            cases.len(),
            differences.len()
        );
        let mut residual: Vec<String> =
            fields.iter().map(|f| format!("{f}-opaque-array")).collect();
        residual.push("unknown-lone-surrogate".into());
        assert_eq!(
            differences, residual,
            "exact remaining identities, not wildcard acceptance"
        );
        eprintln!(
            "PASS: 44 strict source-equivalent Create UTF-8/object cases; 8 opaque-field gaps and 1 outer-guarded helper-only difference explicitly asserted"
        );
    }
}

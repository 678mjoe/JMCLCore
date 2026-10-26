//! Legacy local content registry. Immutable content links are renamed/unlinked,
//! never rewritten; mutable registry state is published by atomic replacement.
use crate::protocol::RpcError;
use serde::Serialize;
use serde_json::{Map, Value, value::RawValue};
use std::io;
pub mod adopt;
mod cli;
pub mod cli_remote;
pub mod curseforge;
mod fs;
pub mod http;
pub mod modrinth;
pub mod remote;
pub mod rpc;
pub use cli::run as cli;
pub mod registry;
use registry::{Entry, Kind, Registry};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid,
    Selector(&'static str),
    Registry(&'static str),
    NotFound,
    WorldInvalid,
    WorldNotFound,
    WorldLocked,
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
type Result<T> = std::result::Result<T, Error>;

/// Zig serializes valid UTF-8 paths as strings and other path bytes as arrays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPath(pub Vec<u8>);
impl Serialize for ListedPath {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(s) => serializer.serialize_str(s),
            Err(_) => self.0.serialize(serializer),
        }
    }
}

pub struct Root(fs::Dir);
impl Root {
    #[cfg(target_os = "linux")]
    pub(crate) fn from_directory(dir: std::fs::File) -> Self {
        Self(fs::Dir::from_directory(dir))
    }
    pub fn open(path: &str) -> Result<Self> {
        Ok(Self(fs::Dir::root(path)?))
    }
    pub fn world(game: &str, name: &str, mutable: bool) -> Result<Self> {
        let game = fs::Dir::root(game)?;
        let saves = game.child("saves", false).map_err(world_error)?;
        if !valid_world(name) {
            return Err(Error::WorldInvalid);
        }
        let world = saves.child(name, false).map_err(world_error)?;
        if mutable && world.locked()? {
            return Err(Error::WorldLocked);
        }
        Ok(Self(world))
    }
    pub fn load(&self) -> Result<Registry> {
        self.load_with(registry::parse)
    }
    fn load_with<F: registry::RegistryFile>(
        &self,
        parse: fn(&[u8]) -> std::result::Result<registry::RegistryRecord<F>, &'static str>,
    ) -> Result<registry::RegistryRecord<F>> {
        let metadata = match self.0.child(".jmcl", false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(registry::RegistryRecord::default());
            }
            Err(e) => return Err(e.into()),
        };
        let bytes = match metadata.read("content.json", registry::MAX_BYTES) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(registry::RegistryRecord::default());
            }
            Err(_) => return Err(Error::Registry("ContentRegistryTooLarge")),
        };
        parse(&bytes).map_err(Error::Registry)
    }
    pub fn save(&self, r: &Registry) -> Result<()> {
        self.save_record(r)
    }
    fn save_record<F: registry::RegistryFile>(
        &self,
        r: &registry::RegistryRecord<F>,
    ) -> Result<()> {
        r.validate().map_err(Error::Registry)?;
        let mut data = serde_json::to_vec(r).map_err(|_| Error::Registry("Unexpected"))?;
        data.push(b'\n');
        self.0
            .child(".jmcl", true)?
            .atomic_write("content.json", &data)?;
        Ok(())
    }
    pub fn list(&self, kind: Kind) -> Result<(Vec<Entry>, Vec<String>)> {
        // Preserve the existing String-only adoption/mutation view, including
        // rejection of raw registered paths; listing must not widen this path.
        let (entries, paths) = self.list_with_registry(kind, self.load()?)?;
        // Adoption still consumes the legacy UTF-8 path API. Listing must not
        // silently discard names merely because adoption cannot yet represent them.
        Ok((
            entries,
            paths
                .into_iter()
                .filter_map(|p| String::from_utf8(p.0).ok())
                .collect(),
        ))
    }
    pub fn list_paths(&self, kind: Kind) -> Result<(Vec<registry::ListingEntry>, Vec<ListedPath>)> {
        self.list_with_registry(kind, self.load_with(registry::parse_listing)?)
    }
    fn list_with_registry<F: registry::RegistryFile>(
        &self,
        kind: Kind,
        registry: registry::RegistryRecord<F>,
    ) -> Result<(Vec<registry::EntryRecord<F>>, Vec<ListedPath>)> {
        let entries = registry
            .entries
            .iter()
            .filter(|e| e.kind == kind)
            .cloned()
            .collect();
        let dir = match self.0.child(kind.directory(), false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((entries, vec![])),
            Err(e) => return Err(e.into()),
        };
        let mut unmanaged = vec![];
        for (name, is_file) in dir.names()? {
            if !is_file || !name.ends_with(kind.extension().as_bytes()) {
                continue;
            }
            let mut file = kind.directory().as_bytes().to_vec();
            file.push(b'/');
            file.extend_from_slice(&name);
            if !registry.entries.iter().any(|e| e.file.file_bytes() == file) {
                unmanaged.push(ListedPath(file))
            }
        }
        Ok((entries, unmanaged))
    }
    fn selected<F: registry::RegistryFile>(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&[u8]>,
        remove: bool,
        parse: fn(&[u8]) -> std::result::Result<registry::RegistryRecord<F>, &'static str>,
    ) -> Result<(registry::RegistryRecord<F>, usize)> {
        if project.is_some() == file.is_some() {
            return Err(Error::Invalid);
        }
        if project.is_some_and(|p| !registry::project_ref(p)) {
            return Err(Error::Selector("InvalidModrinthProject"));
        }
        if file.is_some_and(|p| !registry::relative_path_bytes(p)) {
            return Err(Error::Selector("UnsafePath"));
        }
        if file.is_some_and(|p| p.len() > 512) {
            return Err(Error::Selector("InvalidContentFile"));
        }
        let r = self.load_with(parse)?;
        let index = r
            .entries
            .iter()
            .position(|e| {
                e.kind == kind
                    && match project {
                        Some(p) => {
                            (!remove || e.source.provider == registry::Provider::Modrinth)
                                && (e.source.project_id.as_deref() == Some(p)
                                    || e.source.slug.as_deref() == Some(p))
                        }
                        None => Some(e.file.file_bytes()) == file,
                    }
            })
            .ok_or(Error::NotFound)?;
        Ok((r, index))
    }
    pub fn remove(&self, kind: Kind, project: Option<&str>, file: Option<&str>) -> Result<Entry> {
        self.remove_with(kind, project, file.map(str::as_bytes), registry::parse)
    }
    pub(crate) fn remove_paths(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&str>,
    ) -> Result<registry::ListingEntry> {
        self.remove_bytes(kind, project, file.map(str::as_bytes))
    }
    pub(crate) fn remove_bytes(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&[u8]>,
    ) -> Result<registry::ListingEntry> {
        self.remove_with(kind, project, file, registry::parse_listing)
    }
    fn remove_with<F: registry::RegistryFile>(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&[u8]>,
        parse: fn(&[u8]) -> std::result::Result<registry::RegistryRecord<F>, &'static str>,
    ) -> Result<registry::EntryRecord<F>> {
        let (mut r, index) = self.selected(kind, project, file, true, parse)?;
        let e = r.entries.remove(index);
        let mut path = e.file.file_bytes().to_vec();
        if !e.enabled {
            path.extend_from_slice(b".disabled");
        }
        match self.0.parent_bytes(&path) {
            Ok((parent, name)) => match parent.unlink_bytes(&name) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        self.save_record(&r)?;
        Ok(e)
    }
    pub fn toggle(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&str>,
        enabled: bool,
    ) -> Result<(bool, Entry)> {
        self.toggle_with(
            kind,
            project,
            file.map(str::as_bytes),
            enabled,
            registry::parse,
        )
    }
    pub(crate) fn toggle_paths(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&str>,
        enabled: bool,
    ) -> Result<(bool, registry::ListingEntry)> {
        self.toggle_bytes(kind, project, file.map(str::as_bytes), enabled)
    }
    pub(crate) fn toggle_bytes(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&[u8]>,
        enabled: bool,
    ) -> Result<(bool, registry::ListingEntry)> {
        self.toggle_with(kind, project, file, enabled, registry::parse_listing)
    }
    fn toggle_with<F: registry::RegistryFile>(
        &self,
        kind: Kind,
        project: Option<&str>,
        file: Option<&[u8]>,
        enabled: bool,
        parse: fn(&[u8]) -> std::result::Result<registry::RegistryRecord<F>, &'static str>,
    ) -> Result<(bool, registry::EntryRecord<F>)> {
        let (mut r, index) = self.selected(kind, project, file, false, parse)?;
        let e = &r.entries[index];
        match self.0.parent_bytes(e.file.file_bytes()) {
            Ok((parent, name)) => {
                let disabled = [name.as_slice(), b".disabled"].concat();
                let canonical = parent.exists_bytes(&name)?;
                let off = parent.exists_bytes(&disabled)?;
                if enabled && !canonical && off {
                    parent.rename_bytes(&disabled, &name)?
                } else if !enabled && canonical {
                    parent.rename_bytes(&name, &disabled)?
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        let changed = e.enabled != enabled;
        r.entries[index].enabled = enabled;
        if changed {
            self.save_record(&r)?
        }
        Ok((changed, r.entries.remove(index)))
    }
}
fn world_error(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::NotFound {
        Error::WorldNotFound
    } else {
        Error::Io(e)
    }
}
fn valid_world(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 255
        && !n.starts_with('.')
        && !n.ends_with([' ', '.'])
        && !n.bytes().any(|b| b == b'/' || b == b'\\' || b < 32)
}

struct Params {
    directory: String,
    world: Option<String>,
    project: Option<String>,
    file: Option<Vec<u8>>,
}
/// Only local content's file selector uses this dynamic byte-string boundary.
/// Numeric JSON elements follow binary64 Value coercion; quoted numbers keep
/// the typed numeric grammar. Do not use the typed metadata byte visitor here.
pub(crate) fn dynamic_file(raw: &str) -> std::result::Result<Vec<u8>, ()> {
    let raw = raw.trim();
    if raw.starts_with('"') {
        return serde_json::from_str::<String>(raw)
            .map(String::into_bytes)
            .map_err(|_| ());
    }
    let values: Vec<&RawValue> = serde_json::from_str(raw).map_err(|_| ())?;
    values
        .into_iter()
        .map(|v| {
            crate::json_number::dynamic_raw(v.get().trim(), 8)
                .map(|v| v as u8)
                .map_err(|_| ())
        })
        .collect()
}
fn parse_params(raw: &str, world: bool, selector: bool) -> Option<Params> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let o = value.as_object()?;
    let directory = o.get("directory")?.as_str()?.to_string();
    let required = |key: &str| -> Option<String> { Some(o.get(key)?.as_str()?.to_string()) };
    let optional = |key: &str| -> Option<Option<String>> {
        match o.get(key) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(v)) => Some(Some(v.clone())),
            _ => None,
        }
    };
    Some(Params {
        directory,
        world: if world {
            Some(required("world")?)
        } else {
            None
        },
        project: if selector { optional("project")? } else { None },
        file: if selector {
            // Match the existing Value map's last-key-wins semantics while
            // preserving the file element lexemes for dynamic u8 conversion.
            let fields: std::collections::BTreeMap<String, &RawValue> =
                serde_json::from_str(raw).ok()?;
            match fields.get("file") {
                None => None,
                Some(v) if v.get().trim() == "null" => None,
                Some(v) => Some(dynamic_file(v.get()).ok()?),
            }
        } else {
            None
        },
    })
}
fn invalid(s: impl Into<String>) -> RpcError {
    RpcError::new("INVALID_PARAMS", s)
}
fn val<T: Serialize>(v: T) -> Value {
    serde_json::to_value(v).expect("public serializable content")
}
fn result(stage: &str, world: Option<&str>, fields: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    m.insert("stage".into(), val(stage));
    if let Some(w) = world {
        m.insert("world".into(), val(w));
    }
    for (k, v) in fields {
        m.insert(k.into(), v);
    }
    Value::Object(m)
}
pub fn handle(
    method: &str,
    params: Option<&RawValue>,
) -> Option<std::result::Result<Value, RpcError>> {
    let (family, op) = method.split_once('.')?;
    let kind = Kind::from_directory(family)?;
    if op == "adopt" {
        return Some(adopt::handle(kind, params));
    }
    if !matches!(op, "list" | "remove" | "enable" | "disable") {
        return None;
    }
    let datapack = kind == Kind::Datapack;
    let noun = if datapack { "datapack" } else { "content" };
    let action = match op {
        "list" => "listing",
        "remove" => "removal",
        _ => "toggle",
    };
    let need = if op == "list" {
        if datapack {
            "a directory and world"
        } else {
            "a directory"
        }
    } else if datapack {
        "a directory, world, and selector"
    } else {
        "a directory and a selector"
    };
    Some((|| {
        let raw = params.ok_or_else(|| invalid(format!("{noun} {action} requires {need}")))?;
        if !raw.get().trim_start().starts_with('{') {
            return Err(invalid(format!("{noun} {action} parameters are invalid")));
        }
        let p = parse_params(raw.get(), datapack, op != "list")
            .ok_or_else(|| invalid(format!("{noun} {action} parameters are invalid")))?;
        if datapack && p.world.is_none() {
            return Err(invalid(format!("{noun} {action} parameters are invalid")));
        }
        if p.directory.is_empty()
            || (datapack && p.world.as_ref().is_none_or(|w| w.is_empty()))
            || (op != "list" && p.project.is_some() == p.file.is_some())
        {
            let message = if op == "list" {
                if datapack {
                    "datapack listing requires a directory and world"
                } else {
                    "content listing directory must not be empty"
                }
                .to_string()
            } else {
                format!(
                    "{noun} {action} requires {}",
                    if datapack {
                        "a directory, world, and exactly one of project or file"
                    } else {
                        "a directory and exactly one of project or file"
                    }
                )
            };
            return Err(invalid(message));
        }
        let fallback = match op {
            "list" => (
                "CONTENT_LIST_FAILED",
                if datapack {
                    "Failed to read the world datapack registry"
                } else {
                    "Failed to read the instance content registry"
                },
            ),
            "remove" => (
                "CONTENT_REMOVE_FAILED",
                if datapack {
                    "Failed to remove the datapack entry"
                } else {
                    "Failed to remove the content entry"
                },
            ),
            _ => (
                "CONTENT_TOGGLE_FAILED",
                if datapack {
                    "Failed to toggle the datapack entry"
                } else {
                    "Failed to toggle the content entry"
                },
            ),
        };
        let map = |e: Error| match e {
            Error::Registry("UnsafePath" | "InvalidContentFile") if datapack && op == "list" => {
                invalid("The selector is invalid")
            }
            Error::Registry(
                "InvalidModrinthProject"
                | "InvalidModrinthVersionId"
                | "InvalidGameVersion"
                | "InvalidCurseforgeId",
            ) if datapack && op == "list" => {
                invalid("Project, version, or Minecraft version identifiers are invalid")
            }
            Error::Invalid | Error::Selector(_) => invalid("The selector is invalid"),
            Error::NotFound => RpcError::new(
                "CONTENT_NOT_FOUND",
                "No registered content matches the given selector",
            ),
            Error::WorldNotFound => {
                RpcError::new("WORLD_NOT_FOUND", "No world matches the given name")
            }
            Error::WorldLocked => {
                RpcError::new("WORLD_LOCKED", "The world is in use by a running game")
            }
            _ => RpcError::new(fallback.0, fallback.1),
        };
        let world = if datapack { p.world.as_deref() } else { None };
        let root = if datapack {
            Root::world(&p.directory, world.unwrap(), op != "list").map_err(map)?
        } else {
            Root::open(&p.directory).map_err(|_| {
                RpcError::new("INSTALL_ROOT_ERROR", "Failed to open the install root")
            })?
        };
        match op {
            "list" => {
                let (e, u) = root.list_paths(kind).map_err(map)?;
                Ok(result(
                    method,
                    world,
                    vec![("entries", val(e)), ("unmanaged", val(u))],
                ))
            }
            "remove" => {
                let e = root
                    .remove_bytes(kind, p.project.as_deref(), p.file.as_deref())
                    .map_err(map)?;
                Ok(result(method, world, vec![("removed", val(e))]))
            }
            _ => {
                let (c, e) = root
                    .toggle_bytes(
                        kind,
                        p.project.as_deref(),
                        p.file.as_deref(),
                        op == "enable",
                    )
                    .map_err(map)?;
                Ok(result(
                    method,
                    world,
                    vec![("changed", val(c)), ("entry", val(e))],
                ))
            }
        }
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use registry::{Provider, Source};
    use std::fs;
    fn entry(kind: Kind, name: &str, provider: Provider, id: Option<&str>) -> Entry {
        Entry {
            kind,
            file: format!("{}/{name}", kind.directory()),
            sha1: "a".repeat(40),
            size: 1,
            source: Source {
                provider,
                project_id: id.map(str::to_string),
                version_id: id.map(|_| "12".into()),
                slug: None,
            },
            enabled: true,
            as_dependency: false,
        }
    }
    #[test]
    fn toggle_remove_preserve_shared_inode() {
        let t = tempfile::tempdir().unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        fs::create_dir(t.path().join("mods")).unwrap();
        fs::write(t.path().join("object"), b"x").unwrap();
        fs::hard_link(t.path().join("object"), t.path().join("mods/a.jar")).unwrap();
        let e = entry(Kind::Mod, "a.jar", Provider::Manual, None);
        root.save(&Registry {
            entries: vec![e.clone()],
            ..Registry::default()
        })
        .unwrap();
        let (changed, out) = root
            .toggle(Kind::Mod, None, Some("mods/a.jar"), false)
            .unwrap();
        assert!(changed);
        assert!(!out.enabled);
        assert!(t.path().join("mods/a.jar.disabled").exists());
        assert!(
            !root
                .toggle(Kind::Mod, None, Some("mods/a.jar"), false)
                .unwrap()
                .0
        );
        assert_eq!(
            root.remove(Kind::Mod, None, Some("mods/a.jar")).unwrap(),
            out
        );
        assert_eq!(fs::read(t.path().join("object")).unwrap(), b"x");
        assert!(root.load().unwrap().entries.is_empty());
    }
    #[test]
    fn remove_provider_selector_asymmetry_is_preserved() {
        let t = tempfile::tempdir().unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        let a = entry(Kind::Mod, "a.jar", Provider::Curseforge, Some("123"));
        let b = entry(Kind::Mod, "b.jar", Provider::Modrinth, Some("123"));
        root.save(&Registry {
            entries: vec![a, b],
            ..Registry::default()
        })
        .unwrap();
        assert_eq!(
            root.toggle(Kind::Mod, Some("123"), None, false)
                .unwrap()
                .1
                .file,
            "mods/a.jar"
        );
        assert_eq!(
            root.remove(Kind::Mod, Some("123"), None).unwrap().file,
            "mods/b.jar"
        );
        assert!(matches!(
            root.remove(Kind::Mod, Some("123"), None),
            Err(Error::NotFound)
        ));
    }
    #[test]
    fn missing_files_and_partial_toggles_are_recoverable() {
        let t = tempfile::tempdir().unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        let e = entry(Kind::Mod, "a.jar", Provider::Manual, None);
        root.save(&Registry {
            entries: vec![e],
            ..Registry::default()
        })
        .unwrap();
        assert!(
            root.toggle(Kind::Mod, None, Some("mods/a.jar"), false)
                .unwrap()
                .0
        );
        fs::create_dir(t.path().join("mods")).unwrap();
        fs::write(t.path().join("mods/a.jar"), b"x").unwrap();
        assert!(
            !root
                .toggle(Kind::Mod, None, Some("mods/a.jar"), false)
                .unwrap()
                .0
        );
        assert!(t.path().join("mods/a.jar.disabled").exists());
        root.remove(Kind::Mod, None, Some("mods/a.jar")).unwrap();
    }
    #[test]
    fn registry_validation_and_provider_identity() {
        let a = entry(Kind::Mod, "a.jar", Provider::Modrinth, Some("123"));
        let mut b = entry(Kind::Mod, "b.jar", Provider::Curseforge, Some("123"));
        let mut r = Registry {
            entries: vec![a, b.clone()],
            ..Registry::default()
        };
        assert!(r.validate().is_ok());
        b.source.provider = Provider::Modrinth;
        r.entries[1] = b;
        assert_eq!(r.validate(), Err("DuplicateContentProject"));
        r.entries[1].source.project_id = Some("456".into());
        r.entries[1].file = "mods/../outside.jar".into();
        assert_eq!(r.validate(), Err("UnsafePath"));
    }
    #[test]
    fn datapack_mutations_reject_live_lock_allow_stale() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("saves/World/datapacks")).unwrap();
        let root = Root::world(t.path().to_str().unwrap(), "World", false).unwrap();
        root.save(&Registry {
            entries: vec![entry(Kind::Datapack, "a.zip", Provider::Manual, None)],
            ..Registry::default()
        })
        .unwrap();
        fs::write(t.path().join("saves/World/datapacks/a.zip"), b"x").unwrap();
        let f = fs::File::create(t.path().join("saves/World/session.lock")).unwrap();
        f.lock().unwrap();
        assert!(matches!(
            Root::world(t.path().to_str().unwrap(), "World", true),
            Err(Error::WorldLocked)
        ));
        assert_eq!(root.list(Kind::Datapack).unwrap().0.len(), 1);
        f.unlock().unwrap();
        Root::world(t.path().to_str().unwrap(), "World", true)
            .unwrap()
            .remove(Kind::Datapack, None, Some("datapacks/a.zip"))
            .unwrap();
    }
    #[test]
    fn listing_does_not_adopt_or_follow_unmanaged_links() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir(t.path().join("mods")).unwrap();
        fs::write(t.path().join("mods/a.jar"), b"x").unwrap();
        fs::write(t.path().join("mods/a.jar.disabled"), b"x").unwrap();
        fs::create_dir(t.path().join("mods/folder.jar")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("a.jar", t.path().join("mods/link.jar")).unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        assert_eq!(root.list(Kind::Mod).unwrap().1, vec!["mods/a.jar"]);
        assert!(!t.path().join(".jmcl/content.json").exists());
    }
}

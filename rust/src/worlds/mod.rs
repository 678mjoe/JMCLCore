//! Client worlds are mutable private state: no CAS or hard-link publication.
//! Platform adapters preserve private mutable copies and bounded metadata parsing.
mod archive;
mod cli;
mod cli_read;
pub use cli_read::read_cli_os;
#[cfg(unix)]
pub(crate) mod fs;
#[cfg(windows)]
#[path = "windows.rs"]
pub(crate) mod fs;
mod nbt;
mod read;
mod rpc;
pub use cli::cli;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
pub use rpc::handle;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::{io, path::Path};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    IconIo,
    TooManyWorlds,
    InvalidName,
    WorldNotFound,
    WorldExists,
    WorldLocked,
    InvalidLevelData,
    LevelDataMissing,
    IconTooLarge,
    UnsupportedEntry,
    BackupNotFound,
    ArchiveTooLarge,
    ArchiveInvalid,
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
pub const MAX_ARCHIVE: usize = 1024 * 1024 * 1024;
pub fn validate_name(n: &str) -> Result<()> {
    if n.is_empty()
        || n.len() > 255
        || n.starts_with('.')
        || n.ends_with([' ', '.'])
        || n.bytes()
            .any(|b| b == 0 || b < 32 || b == b'/' || b == b'\\')
    {
        Err(Error::InvalidName)
    } else {
        Ok(())
    }
}
pub fn is_temp(n: &str) -> bool {
    n.contains(".jmcl-copy-") || n.contains(".jmcl-trash-")
}
fn unique(marker: &str, name: &str) -> Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| Error::Io(io::Error::other(e.to_string())))?;
    Ok(format!("{marker}{name}{:016x}", u64::from_le_bytes(bytes)))
}

fn absent(e: Error) -> Error {
    match e {
        Error::Io(e) if e.kind() == io::ErrorKind::NotFound => Error::WorldNotFound,
        _ => e,
    }
}
fn decode_level(bytes: &[u8]) -> Result<nbt::Document> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::InvalidLevelData);
    }
    let mut data = vec![];
    GzDecoder::new(bytes)
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|_| Error::InvalidLevelData)?;
    nbt::parse(&data)
}
fn level_meta(doc: Option<&nbt::Document>) -> Value {
    let data = doc.and_then(|d| d.root.get("Data"));
    let get = |n: &str| data.and_then(|d| d.get(n));
    let text = |n: &str| get(n).and_then(nbt::Tag::text);
    let byte = |n: &str| match get(n) {
        Some(nbt::Tag::Byte(v)) => Some(*v),
        _ => None,
    };
    let version = get("Version");
    json!({"level_name":text("LevelName"),"version_name":version.and_then(|v|v.get("Name")).and_then(nbt::Tag::text),"version_id":match version.and_then(|v|v.get("Id")){Some(nbt::Tag::Int(v))=>Some(*v),_=>None},"game_mode":match get("GameType"){Some(nbt::Tag::Int(0))=>Some("survival"),Some(nbt::Tag::Int(1))=>Some("creative"),Some(nbt::Tag::Int(2))=>Some("adventure"),Some(nbt::Tag::Int(3))=>Some("spectator"),_=>None},"hardcore":byte("hardcore").map(|v|v!=0),"cheats":byte("allowCommands").map(|v|v!=0),"difficulty":byte("Difficulty").map(|v|v as u8),"last_played_ms":match get("LastPlayed"){Some(nbt::Tag::Long(v))=>Some(*v),_=>None}})
}
fn world(parent: &fs::Dir, name: &str, version: Option<&str>) -> Result<Value> {
    validate_name(name)?;
    let d = parent.child_alias(name).map_err(absent)?;
    let doc = d
        .read_alias("level.dat", 32 * 1024 * 1024)
        .ok()
        .and_then(|b| decode_level(&b).ok());
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(name));
    for (k, v) in level_meta(doc.as_ref()).as_object().unwrap() {
        obj.insert(k.clone(), v.clone());
    }
    obj.insert("size_bytes".into(), json!(d.size()));
    obj.insert("has_icon".into(), json!(d.exists("icon.png")));
    obj.insert("locked".into(), json!(d.locked().unwrap_or(true)));
    let a = obj.get("version_name").and_then(Value::as_str);
    let relation = match (a, version) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => {
            if a == b {
                "same"
            } else {
                "different"
            }
        }
        _ => "unknown",
    };
    obj.insert("version_relation".into(), json!(relation));
    Ok(Value::Object(obj))
}
pub(crate) fn saves(path: &str) -> Result<fs::Dir> {
    fs::Dir::root(Path::new(path))?
        .child("saves", false)
        .map_err(absent)
}
pub(crate) fn guard(parent: &fs::Dir, name: &str) -> Result<fs::Dir> {
    validate_name(name)?;
    let d = parent.child_alias(name).map_err(absent)?;
    if d.locked()? {
        return Err(Error::WorldLocked);
    }
    Ok(d)
}
fn cleanup(parent: &fs::Dir) -> Result<()> {
    for (n, t) in parent.names()? {
        if t == fs::FileType::Directory && n.to_str().is_some_and(is_temp) {
            let _ = parent.remove_tree(n);
        }
    }
    Ok(())
}
pub fn list(path: &str, version: Option<&str>) -> Result<Value> {
    let s = match saves(path) {
        Ok(s) => s,
        Err(Error::WorldNotFound) => return Ok(json!({"stage":"worlds.list","worlds":[]})),
        Err(e) => return Err(e),
    };
    cleanup(&s)?;
    let mut names = s
        .names()?
        .into_iter()
        .filter(|(n, t)| {
            *t == fs::FileType::Directory
                && n.to_str()
                    .is_some_and(|n| !is_temp(n) && validate_name(n).is_ok())
        })
        .filter_map(|(n, _)| n.into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    let mut worlds = vec![];
    for n in names {
        if worlds.len() >= 1024 {
            return Err(Error::TooManyWorlds);
        }
        if let Ok(w) = world(&s, &n, version) {
            worlds.push(w)
        }
    }
    Ok(json!({"stage":"worlds.list","worlds":worlds}))
}
pub fn get(path: &str, name: &str, version: Option<&str>) -> Result<Value> {
    use base64::Engine;
    let s = saves(path)?;
    let w = world(&s, name, version)?;
    let icon = match s.child_alias(name)?.read_alias("icon.png", 65536) {
        Ok(b) => Some(base64::engine::general_purpose::STANDARD.encode(b)),
        Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => None,
        Err(Error::ArchiveTooLarge) => return Err(Error::IconTooLarge),
        Err(_) => return Err(Error::IconIo),
    };
    Ok(json!({"stage":"worlds.get","world":w,"icon_png_base64":icon}))
}
fn set_level_name(d: &fs::Dir, new: &str) -> Result<()> {
    let bytes = d
        .read_alias("level.dat", 32 * 1024 * 1024)
        .map_err(|e| match e {
            Error::Io(e) if e.kind() == io::ErrorKind::NotFound => Error::LevelDataMissing,
            Error::ArchiveTooLarge => Error::InvalidLevelData,
            _ => e,
        })?;
    let mut doc = decode_level(&bytes)?;
    let Some(nbt::Tag::String(name)) = doc
        .root
        .get_mut("Data")
        .and_then(|d| d.get_mut("LevelName"))
    else {
        return Err(Error::InvalidLevelData);
    };
    *name = new.as_bytes().to_vec();
    let bytes = nbt::serialize(&doc)?;
    if !nbt::equivalent(&doc.root, &nbt::parse(&bytes)?.root) {
        return Err(Error::InvalidLevelData);
    }
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&bytes)?;
    d.write_atomic("level.dat", &enc.finish()?)
}
pub fn rename(path: &str, old: &str, new: &str, version: Option<&str>) -> Result<Value> {
    if old == new {
        return Err(Error::InvalidName);
    }
    let s = saves(path)?;
    rename_in(&s, old, new, version)
}
pub(crate) fn rename_in(s: &fs::Dir, old: &str, new: &str, version: Option<&str>) -> Result<Value> {
    if old == new {
        return Err(Error::InvalidName);
    }
    let d = guard(s, old)?;
    validate_name(new)?;
    if s.exists(new) {
        return Err(Error::WorldExists);
    }
    match set_level_name(&d, new) {
        Ok(()) | Err(Error::InvalidLevelData) | Err(Error::LevelDataMissing) => {}
        Err(e) => return Err(e),
    }
    drop(d);
    s.rename(old, s, new)?;
    Ok(json!({"stage":"worlds.rename","world":world(s,new,version)?}))
}
pub fn duplicate(path: &str, old: &str, new: &str, version: Option<&str>) -> Result<Value> {
    if old == new {
        return Err(Error::InvalidName);
    }
    let s = saves(path)?;
    let d = guard(&s, old)?;
    validate_name(new)?;
    if s.exists(new) {
        return Err(Error::WorldExists);
    }
    let tmp = unique(".jmcl-copy-", new)?;
    let to = s.new_child(&tmp)?;
    let r = (|| {
        d.copy_to(&to)?;
        drop(to);
        s.rename(&tmp, &s, new)?;
        Ok(json!({"stage":"worlds.duplicate","world":world(&s,new,version)?}))
    })();
    if r.is_err() {
        let _ = s.remove_tree(&tmp);
    }
    r
}
pub fn delete(path: &str, name: &str) -> Result<Value> {
    let s = saves(path)?;
    delete_in(&s, name)
}
pub(crate) fn delete_in(s: &fs::Dir, name: &str) -> Result<Value> {
    drop(guard(s, name)?);
    let trash = unique(".jmcl-trash-", name)?;
    s.rename(name, s, &trash)?;
    if let Err(e) = s.remove_tree(&trash) {
        let _ = s.rename(&trash, s, name);
        return Err(e);
    }
    Ok(json!({"stage":"worlds.delete","deleted":name}))
}
fn validate_backup(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 327 || !name.ends_with(".zip") || name.contains(['/', '\\'])
    {
        Err(Error::InvalidName)
    } else {
        Ok(())
    }
}
pub fn sanitize_label(s: &str) -> Option<String> {
    let mut out = String::new();
    for b in s.bytes() {
        if out.len() >= 40 {
            break;
        }
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(b as char)
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-')
        }
    }
    let s = out.trim_end_matches(['-', '.']);
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}
fn backup_world_name(name: &str) -> Option<&str> {
    let b = name.strip_suffix(".zip")?;
    for (i, c) in b.bytes().enumerate() {
        if c != b'-' || i + 16 > b.len() {
            continue;
        }
        let marker = &b.as_bytes()[i + 1..i + 16];
        if marker[..8].iter().all(u8::is_ascii_digit)
            && marker[8] == b'-'
            && marker[9..].iter().all(u8::is_ascii_digit)
            && (i + 16 == b.len() || b.as_bytes()[i + 16] == b'-')
            && i > 0
        {
            return b.get(..i);
        }
    }
    None
}
pub fn backups(path: &str) -> Result<Value> {
    let root = fs::Dir::root(Path::new(path))?;
    backups_in(&root)
}
pub(crate) fn backups_in(root: &fs::Dir) -> Result<Value> {
    let b = match root.child("backups", false) {
        Ok(b) => b,
        Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(json!({"stage":"worlds.backups","backups":[]}));
        }
        Err(e) => return Err(e),
    };
    let mut entries = vec![];
    for (n, t) in b.names()? {
        if t != fs::FileType::RegularFile {
            continue;
        }
        let Some(n) = n.to_str().filter(|n| n.ends_with(".zip")) else {
            continue;
        };
        if entries.len() >= 1024 {
            return Err(Error::TooManyWorlds);
        }
        let m = b.file(n, false)?.metadata()?;
        let millis = m
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_else(|e| -(e.duration().as_millis() as i64));
        entries.push(
            json!({"file":n,"world":backup_world_name(n),"created_ms":millis,"size_bytes":m.len()}),
        );
    }
    entries.sort_by_key(|v| std::cmp::Reverse(v["created_ms"].as_i64().unwrap_or(0)));
    Ok(json!({"stage":"worlds.backups","backups":entries}))
}
pub fn delete_backup(path: &str, name: &str) -> Result<Value> {
    validate_backup(name)?;
    let b = fs::Dir::root(Path::new(path))?.child("backups", false)?;
    b.unlink(name).map_err(|e| match e {
        Error::Io(e) if e.kind() == io::ErrorKind::NotFound => Error::BackupNotFound,
        _ => e,
    })?;
    Ok(json!({"stage":"worlds.backups.delete","deleted":name}))
}
#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
pub mod server;

pub mod server_cli;
mod server_portable_cli;

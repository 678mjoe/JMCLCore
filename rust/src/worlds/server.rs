//! Server worlds share the client format, but are guarded by server lifecycle
//! and coupled to level-name plus optional Nether/End sibling directories.
use super::*;
use crate::{
    protocol::RpcError,
    servers::{lifecycle, properties, store},
};
use serde_json::value::RawValue;
#[path = "server_read.rs"]
mod read_params;
fn error(code: &str, message: &str) -> RpcError {
    RpcError::new(code, message)
}
fn level_name(dir: &fs::Dir) -> Result<(properties::Document, String)> {
    let bytes = lifecycle::read(&dir.0, "server.properties", properties::MAX_FILE_BYTES)?;
    let doc = properties::parse(&bytes)
        .map_err(|_| Error::Io(io::Error::other("InvalidServerProperties")))?;
    let name = doc
        .get("level-name")
        .ok_or_else(|| Error::Io(io::Error::other("InvalidServerProperties")))?
        .to_owned();
    Ok((doc, name))
}
pub fn handle(
    method: &str,
    params: Option<&RawValue>,
) -> Option<std::result::Result<Value, RpcError>> {
    if matches!(method, "server.worlds.list" | "server.worlds.get") {
        return Some(read_params::handle(method, params));
    }
    let (what, code, message) = match method {
        "server.worlds.list" => (
            "server worlds listing",
            "WORLD_LIST_FAILED",
            "Failed to list server worlds",
        ),
        "server.worlds.get" => (
            "server world lookup",
            "WORLD_GET_FAILED",
            "Failed to inspect the server world",
        ),
        "server.worlds.rename" => (
            "server world rename",
            "WORLD_RENAME_FAILED",
            "Failed to rename the server world",
        ),
        "server.worlds.delete" => (
            "server world deletion",
            "WORLD_DELETE_FAILED",
            "Failed to delete the server world",
        ),
        "server.worlds.backups" => (
            "server backups listing",
            "WORLD_BACKUP_FAILED",
            "Failed to list backups",
        ),
        "server.worlds.backups.delete" => (
            "server backup deletion",
            "WORLD_BACKUP_FAILED",
            "Failed to delete the backup",
        ),
        "server.worlds.backup" => (
            "server worlds backup",
            "WORLD_BACKUP_FAILED",
            "Failed to back up the server worlds",
        ),
        "server.worlds.restore" => (
            "server worlds restore",
            "WORLD_RESTORE_FAILED",
            "Failed to restore the backup",
        ),
        _ => return None,
    };
    Some((|| {
        let raw = params.ok_or_else(|| error("INVALID_PARAMS", "missing parameters"))?;
        let invalid = || error("INVALID_PARAMS", &format!("{what} parameters are invalid"));
        let value: Value = serde_json::from_str(raw.get()).map_err(|_| invalid())?;
        let obj = value.as_object().ok_or_else(invalid)?;
        let string = |name: &str, default: Option<&str>| -> std::result::Result<String, RpcError> {
            match obj.get(name) {
                Some(Value::String(s)) => Ok(s.clone()),
                None => default.map(str::to_owned).ok_or_else(invalid),
                _ => Err(invalid()),
            }
        };
        let directory = string("directory", Some("servers"))?;
        let id = string("id", None)?;
        let version = if matches!(
            method,
            "server.worlds.list"
                | "server.worlds.get"
                | "server.worlds.delete"
                | "server.worlds.backups"
        ) {
            match obj.get("minecraft_version") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.as_str()),
                _ => return Err(invalid()),
            }
        } else {
            None
        };
        let world = if matches!(
            method,
            "server.worlds.get" | "server.worlds.rename" | "server.worlds.delete"
        ) {
            Some(string("world", None)?)
        } else {
            None
        };
        let new = if method == "server.worlds.rename" {
            Some(string("new_name", None)?)
        } else {
            None
        };
        let backup = if matches!(
            method,
            "server.worlds.backups.delete" | "server.worlds.restore"
        ) {
            Some(string("backup", None)?)
        } else {
            None
        };
        let optional = |name: &str| -> std::result::Result<Option<String>, RpcError> {
            match obj.get(name) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.clone())),
                _ => Err(invalid()),
            }
        };
        let backup_world = if method == "server.worlds.backup" {
            optional("world")?
        } else {
            None
        };
        let label = if method == "server.worlds.backup" {
            optional("label")?
        } else {
            None
        };
        let replace = if method == "server.worlds.restore" {
            match obj.get("replace") {
                None => false,
                Some(Value::Bool(b)) => *b,
                _ => return Err(invalid()),
            }
        } else {
            false
        };
        let suffix = if new.is_some() {
            "directory, id, world, and new_name"
        } else if world.is_some() {
            "directory, id, and world"
        } else if backup.is_some() {
            "directory, id, and backup"
        } else {
            "directory and id"
        };
        if directory.is_empty()
            || id.is_empty()
            || world.as_ref().is_some_and(String::is_empty)
            || new.as_ref().is_some_and(String::is_empty)
            || backup.as_ref().is_some_and(String::is_empty)
        {
            return Err(error(
                "INVALID_PARAMS",
                &format!("{what} requires {suffix}"),
            ));
        }
        if method == "server.worlds.restore" {
            validate_backup(backup.as_deref().unwrap())
                .map_err(|e| super::rpc::map(e, code, message))?;
        }
        let readonly = matches!(method, "server.worlds.list" | "server.worlds.get");
        let root = store::open_root(&directory).map_err(|_| {
            if readonly {
                error("SERVERS_ROOT_ERROR", "Failed to open the servers root")
            } else {
                error("SERVER_NOT_FOUND", "No server matches the given id")
            }
        })?;
        let loaded = if readonly {
            store::load_read(&root, &id).map(|_| ())
        } else {
            store::load(&root, &id).map(|_| ())
        };
        loaded.map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
                error("SERVER_NOT_FOUND", "No server matches the given id")
            } else if readonly {
                error("SERVER_LOAD_FAILED", "Failed to load the server")
            } else {
                error(code, message)
            }
        })?;
        let dir = fs::Dir(store::open_server(&root, &id).map_err(|_| {
            if readonly {
                error("SERVER_ROOT_ERROR", "Failed to open the server directory")
            } else {
                error("SERVER_NOT_FOUND", "No server matches the given id")
            }
        })?);
        let _guard = if matches!(
            method,
            "server.worlds.rename"
                | "server.worlds.delete"
                | "server.worlds.backup"
                | "server.worlds.restore"
        ) {
            Some(lifecycle::lock_stopped(&dir.0).map_err(|e| {
                if e == "ServerAlreadyRunning" {
                    error(
                        "SERVER_ALREADY_RUNNING",
                        "Stop the server before modifying its worlds",
                    )
                } else {
                    error(code, message)
                }
            })?)
        } else {
            None
        };
        let result = (|| -> Result<Value> {
            match method {
                "server.worlds.list" => {
                    let level = level_name(&dir)
                        .map(|(_, n)| n)
                        .unwrap_or_else(|_| "world".into());
                    let mut worlds = vec![];
                    for n in [
                        level.clone(),
                        format!("{level}_nether"),
                        format!("{level}_the_end"),
                    ] {
                        if validate_name(&n).is_err() {
                            continue;
                        }
                        match super::read::world_read(
                            &dir,
                            n.as_bytes(),
                            version.map(str::as_bytes),
                        ) {
                            Ok(w) => worlds.push(w),
                            Err(Error::WorldNotFound) => {}
                            Err(Error::Io(e)) if e.raw_os_error() == Some(20) => {}
                            Err(e) => return Err(e),
                        }
                    }
                    Ok(json!({"stage":method,"id":id,"worlds":worlds}))
                }
                "server.worlds.get" => {
                    use base64::Engine;
                    let name = world.as_deref().unwrap();
                    let w =
                        super::read::world_read(&dir, name.as_bytes(), version.map(str::as_bytes))?;
                    let icon = dir
                        .child(name, false)?
                        .read("icon.png", 65536)
                        .ok()
                        .map(|b| base64::engine::general_purpose::STANDARD.encode(b));
                    Ok(json!({"stage":method,"id":id,"world":w,"icon_png_base64":icon}))
                }
                "server.worlds.rename" => {
                    let old = world.as_deref().unwrap();
                    let new = new.as_deref().unwrap();
                    let (mut props, level) = level_name(&dir)?;
                    rename_in(&dir, old, new, None)?;
                    if old == level {
                        for suffix in ["_nether", "_the_end"] {
                            let old = format!("{old}{suffix}");
                            if dir.exists(&old) {
                                dir.rename(&old, &dir, &format!("{new}{suffix}"))?;
                            }
                        }
                        props
                            .set("level-name", new)
                            .map_err(|_| Error::Io(io::Error::other("InvalidServerProperties")))?;
                        lifecycle::atomic_write(&dir.0, "server.properties", &props.encode())?;
                    }
                    Ok(json!({"stage":method,"renamed":{"old_name":old,"new_name":new}}))
                }
                "server.worlds.delete" => {
                    let name = world.as_deref().unwrap();
                    let (_, level) = level_name(&dir)?;
                    delete_in(&dir, name)?;
                    if name == level {
                        for suffix in ["_nether", "_the_end"] {
                            let _ = dir.remove_tree(format!("{name}{suffix}"));
                        }
                    }
                    Ok(json!({"stage":method,"deleted":name}))
                }
                "server.worlds.backups" => {
                    let result = backups_in(&dir)?;
                    Ok(json!({"stage":method,"id":id,"backups":result["backups"]}))
                }
                "server.worlds.backups.delete" => {
                    let backup = backup.as_deref().unwrap();
                    let b = dir.child("backups", false).map_err(|e| match e {
                        Error::Io(e) if e.kind() == io::ErrorKind::NotFound => {
                            Error::BackupNotFound
                        }
                        _ => e,
                    })?;
                    validate_backup(backup)?;
                    b.unlink(backup).map_err(|e| match e {
                        Error::Io(e) if e.kind() == io::ErrorKind::NotFound => {
                            Error::BackupNotFound
                        }
                        _ => e,
                    })?;
                    Ok(json!({"stage":method,"deleted":backup}))
                }
                "server.worlds.backup" => {
                    let (_, level) = level_name(&dir)?;
                    let names = if let Some(name) = &backup_world {
                        validate_name(name)?;
                        if !dir.exists(name) {
                            return Err(Error::WorldNotFound);
                        }
                        vec![name.clone()]
                    } else {
                        let mut names = vec![level.clone()];
                        for suffix in ["_nether", "_the_end"] {
                            let n = format!("{level}{suffix}");
                            if dir.exists(&n) {
                                names.push(n);
                            }
                        }
                        names
                    };
                    let backups = dir.child("backups", true)?;
                    let now = time::OffsetDateTime::now_utc();
                    let base = backup_world.as_deref().unwrap_or(&level);
                    let mut filename = format!(
                        "{base}-{:04}{:02}{:02}-{:02}{:02}{:02}",
                        now.year(),
                        now.month() as u8,
                        now.day(),
                        now.hour(),
                        now.minute(),
                        now.second()
                    );
                    if let Some(label) = sanitize_label(label.as_deref().unwrap_or("")) {
                        filename.push('-');
                        filename.push_str(&label);
                    }
                    filename.push_str(".zip");
                    if filename.len() > 327 {
                        return Err(Error::InvalidName);
                    }
                    if backups.exists(&filename) {
                        return Err(Error::WorldExists);
                    }
                    archive::export_set(&dir, &names, &backups, &filename)?;
                    Ok(json!({"stage":method,"backup":filename}))
                }
                "server.worlds.restore" => {
                    let backup = backup.as_deref().unwrap();
                    validate_backup(backup)?;
                    let missing = |e| match e {
                        Error::Io(e) if e.kind() == io::ErrorKind::NotFound => {
                            Error::BackupNotFound
                        }
                        _ => e,
                    };
                    let backups = dir.child("backups", false).map_err(missing)?;
                    let file = backups.file(backup, false).map_err(missing)?;
                    archive::import_set(&dir, file, replace)?;
                    Ok(json!({"stage":method,"restored":backup}))
                }
                _ => unreachable!(),
            }
        })();
        result.map_err(|e| super::rpc::map(e, code, message))
    })())
}

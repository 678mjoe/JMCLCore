use super::*;
use crate::protocol::RpcError;
use serde_json::value::RawValue;
pub fn handle(
    method: &str,
    params: Option<&RawValue>,
) -> Option<std::result::Result<Value, RpcError>> {
    if matches!(method, "worlds.list" | "worlds.get") {
        return Some(super::read::handle(method, params));
    }
    let (what, fallback, message) = match method {
        "worlds.list" => ("worlds list", "WORLD_LIST_FAILED", "Failed to list worlds"),
        "worlds.get" => (
            "world lookup",
            "WORLD_GET_FAILED",
            "Failed to inspect the world",
        ),
        "worlds.rename" => (
            "worlds.rename",
            "WORLD_RENAME_FAILED",
            "Failed to rename the world",
        ),
        "worlds.duplicate" => (
            "worlds.duplicate",
            "WORLD_COPY_FAILED",
            "Failed to duplicate the world",
        ),
        "worlds.delete" => (
            "world deletion",
            "WORLD_DELETE_FAILED",
            "Failed to delete the world",
        ),
        "worlds.export" => (
            "world export",
            "WORLD_EXPORT_FAILED",
            "Failed to export the world",
        ),
        "worlds.import" => (
            "world import",
            "WORLD_IMPORT_FAILED",
            "Failed to import the world",
        ),
        "worlds.backup" => (
            "world backup",
            "WORLD_BACKUP_FAILED",
            "Failed to back up the world",
        ),
        "worlds.backups" => (
            "world backups listing",
            "WORLD_BACKUP_FAILED",
            "Failed to list world backups",
        ),
        "worlds.restore" => (
            "world restore",
            "WORLD_RESTORE_FAILED",
            "Failed to restore the backup",
        ),
        "worlds.backups.delete" => (
            "world backup deletion",
            "WORLD_BACKUP_FAILED",
            "Failed to delete the backup",
        ),
        _ => return None,
    };
    Some((|| {
        let Some(params) = params else {
            return Err(RpcError::new("INVALID_PARAMS", "missing parameters"));
        };
        let invalid = || RpcError::new("INVALID_PARAMS", format!("{what} parameters are invalid"));
        let v: Value = serde_json::from_str(params.get()).map_err(|_| invalid())?;
        let p = v.as_object().ok_or_else(invalid)?;
        let get = |name: &str, default: Option<&str>| -> std::result::Result<String, RpcError> {
            match p.get(name) {
                Some(Value::String(s)) => Ok(s.clone()),
                None => default.map(str::to_owned).ok_or_else(invalid),
                _ => Err(invalid()),
            }
        };
        let optional = |name: &str| -> std::result::Result<Option<String>, RpcError> {
            match p.get(name) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.clone())),
                _ => Err(invalid()),
            }
        };
        let default = if matches!(
            method,
            "worlds.export"
                | "worlds.import"
                | "worlds.backup"
                | "worlds.restore"
                | "worlds.backups.delete"
        ) {
            Some("instances")
        } else {
            None
        };
        let directory = get("directory", default)?;
        let version = if matches!(
            method,
            "worlds.export" | "worlds.backup" | "worlds.backups.delete"
        ) {
            None
        } else {
            optional("minecraft_version")?
        };
        let require = |s: &str, message: &str| -> std::result::Result<(), RpcError> {
            if directory.is_empty() || s.is_empty() {
                Err(RpcError::new("INVALID_PARAMS", message))
            } else {
                Ok(())
            }
        };
        let result = match method {
            "worlds.list" => {
                require("ok", "worlds listing directory must not be empty")?;
                list(&directory, version.as_deref())
            }
            "worlds.get" => {
                let w = get("world", None)?;
                require(&w, "world lookup requires directory and world")?;
                get_world(&directory, &w, version.as_deref())
            }
            "worlds.rename" | "worlds.duplicate" => {
                let w = get("world", None)?;
                let n = get("new_name", None)?;
                require(&w, "world mutation requires directory, world, and new_name")?;
                require(&n, "world mutation requires directory, world, and new_name")?;
                if method == "worlds.rename" {
                    rename(&directory, &w, &n, version.as_deref())
                } else {
                    duplicate(&directory, &w, &n, version.as_deref())
                }
            }
            "worlds.delete" => {
                let w = get("world", None)?;
                require(&w, "world deletion requires directory and world")?;
                delete(&directory, &w)
            }
            "worlds.export" => {
                let w = get("world", None)?;
                let f = get("file", None)?;
                require(&w, "world export requires directory, world, and file")?;
                require(&f, "world export requires directory, world, and file")?;
                archive::export(&directory, &w, &f)
            }
            "worlds.import" | "worlds.restore" => {
                let key = if method == "worlds.import" {
                    "file"
                } else {
                    "backup"
                };
                let f = get(key, None)?;
                let n = optional("name")?;
                let replace = match p.get("replace") {
                    None => false,
                    Some(Value::Bool(b)) => *b,
                    _ => return Err(invalid()),
                };
                require(
                    &f,
                    if method == "worlds.import" {
                        "world import requires directory and file"
                    } else {
                        "world restore requires directory and backup"
                    },
                )?;
                if method == "worlds.import" {
                    archive::import(&directory, &f, n.as_deref(), replace, version.as_deref())
                } else {
                    archive::restore(&directory, &f, n.as_deref(), replace, version.as_deref())
                }
            }
            "worlds.backup" => {
                let w = get("world", None)?;
                let label = optional("label")?;
                require(&w, "world backup requires directory and world")?;
                archive::backup(&directory, &w, label.as_deref())
            }
            "worlds.backups" => {
                require("ok", "world backups listing directory must not be empty")?;
                backups(&directory)
            }
            "worlds.backups.delete" => {
                let b = get("backup", None)?;
                require(&b, "world backup deletion requires directory and backup")?;
                delete_backup(&directory, &b)
            }
            _ => unreachable!(),
        };
        result.map_err(|e| map(e, fallback, message))
    })())
}
fn get_world(d: &str, w: &str, v: Option<&str>) -> Result<Value> {
    super::get(d, w, v)
}
pub(super) fn map(e: Error, code: &str, message: &str) -> RpcError {
    let (c, m) = match e {
        Error::InvalidName => ("INVALID_PARAMS", "world name is invalid"),
        Error::WorldNotFound => ("WORLD_NOT_FOUND", "No world matches the given name"),
        Error::WorldExists => ("WORLD_EXISTS", "A world with that name already exists"),
        Error::WorldLocked => ("WORLD_LOCKED", "The world is in use by a running game"),
        Error::InvalidLevelData => (
            "WORLD_INVALID_LEVEL_DATA",
            "The world level.dat is malformed",
        ),
        Error::IconTooLarge => (
            "WORLD_ICON_TOO_LARGE",
            "The world icon exceeds the size limit",
        ),
        Error::UnsupportedEntry => (
            "WORLD_COPY_FAILED",
            "The world contains unsupported entries",
        ),
        Error::BackupNotFound => ("BACKUP_NOT_FOUND", "The backup file does not exist"),
        Error::ArchiveTooLarge => (
            "WORLD_ARCHIVE_TOO_LARGE",
            "The archive exceeds the supported size limits",
        ),
        Error::ArchiveInvalid => (
            "WORLD_ARCHIVE_INVALID",
            "The archive is not a valid world zip",
        ),
        Error::IconIo => ("WORLD_GET_FAILED", "Failed to read the world icon"),
        Error::Io(_) | Error::TooManyWorlds | Error::LevelDataMissing => (code, message),
    };
    RpcError::new(c, m)
}

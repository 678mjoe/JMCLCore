//! Selected-field read-only boundary. Parent module remains Linux-gated.
use super::*;
use crate::worlds::read::Bytes;
use serde::Deserialize;
fn default_directory() -> Bytes {
    Bytes(b"servers".to_vec())
}
#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_directory")]
    directory: Bytes,
    id: Bytes,
    #[serde(default)]
    minecraft_version: Option<Bytes>,
}
#[derive(Deserialize)]
struct GetParams {
    #[serde(default = "default_directory")]
    directory: Bytes,
    id: Bytes,
    world: Bytes,
    #[serde(default)]
    minecraft_version: Option<Bytes>,
}
pub(super) fn handle(
    method: &str,
    params: Option<&RawValue>,
) -> std::result::Result<Value, RpcError> {
    let list = method == "server.worlds.list";
    let (what, code, message) = if list {
        (
            "server worlds listing",
            "WORLD_LIST_FAILED",
            "Failed to list server worlds",
        )
    } else {
        (
            "server world lookup",
            "WORLD_GET_FAILED",
            "Failed to inspect the server world",
        )
    };
    let raw = params.ok_or_else(|| error("INVALID_PARAMS", "missing parameters"))?;
    let invalid = || error("INVALID_PARAMS", &format!("{what} parameters are invalid"));
    let json = crate::auth::json::Json::parse(raw.get()).map_err(|_| invalid())?;
    if !json.0.starts_with('{') {
        return Err(invalid());
    }
    let (directory, id, world, version) = if list {
        let p: ListParams = serde_json::from_str(raw.get()).map_err(|_| invalid())?;
        (p.directory, p.id, None, p.minecraft_version)
    } else {
        let p: GetParams = serde_json::from_str(raw.get()).map_err(|_| invalid())?;
        (p.directory, p.id, Some(p.world), p.minecraft_version)
    };
    if directory.0.is_empty() || id.0.is_empty() || world.as_ref().is_some_and(|w| w.0.is_empty()) {
        return Err(error(
            "INVALID_PARAMS",
            &format!(
                "{what} requires {}",
                if list {
                    "directory and id"
                } else {
                    "directory, id, and world"
                }
            ),
        ));
    }
    let root_error = || error("SERVERS_ROOT_ERROR", "Failed to open the servers root");
    let native = crate::store::path_from_bytes(&directory.0).map_err(|_| root_error())?;
    let root = store::open_root_native(&native).map_err(|_| root_error())?;
    // IDs are ASCII-validated by the unchanged store loader, after root opening.
    let id = std::str::from_utf8(&id.0)
        .map_err(|_| error("SERVER_NOT_FOUND", "No server matches the given id"))?;
    store::load_read(&root, id).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
            error("SERVER_NOT_FOUND", "No server matches the given id")
        } else {
            error("SERVER_LOAD_FAILED", "Failed to load the server")
        }
    })?;
    let dir = fs::Dir(
        store::open_server(&root, id)
            .map_err(|_| error("SERVER_ROOT_ERROR", "Failed to open the server directory"))?,
    );
    let version = version.as_ref().map(|v| v.0.as_slice());
    (|| -> Result<Value> {
        if let Some(world) = world {
            use base64::Engine;
            let w = crate::worlds::read::world_read(&dir, &world.0, version)?;
            let native = crate::store::path_from_bytes(&world.0)?;
            // Pinned readIcon follows explicit named aliases; the server caller
            // suppresses every icon lookup/read error after metadata succeeds.
            let icon = dir
                .child_alias_native(&native)
                .and_then(|world| world.read_alias("icon.png", 65536))
                .ok()
                .map(|b| base64::engine::general_purpose::STANDARD.encode(b));
            Ok(json!({"stage":method,"id":id,"world":w,"icon_png_base64":icon}))
        } else {
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
                match crate::worlds::read::world_read(&dir, n.as_bytes(), version) {
                    Ok(w) => worlds.push(w),
                    Err(Error::WorldNotFound) => {}
                    Err(Error::Io(e)) if e.raw_os_error() == Some(20) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(json!({"stage":method,"id":id,"worlds":worlds}))
        }
    })()
    .map_err(|e| crate::worlds::rpc::map(e, code, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_read_params_require_objects_and_known_fields() {
        for method in ["server.worlds.list", "server.worlds.get"] {
            for raw in [
                "[]",
                "[\"servers\",\"demo\"]",
                "true",
                "null",
                "42",
                "{}",
                "{\"id\":true}",
                "{\"id\":\"demo\",\"directory\":null}",
            ] {
                let raw = RawValue::from_string(raw.to_owned()).unwrap();
                let error = handle(method, Some(&raw)).unwrap_err();
                assert_eq!(error.code, "INVALID_PARAMS");
            }
        }
    }
    #[test]
    fn selected_read_byte_grammar_and_defaults_preserve_lexemes() {
        let parsed: ListParams=serde_json::from_str(r#"{"id":[100.0000000000000001,101,109,111],"minecraft_version":["4_9",46,50,48],"unused":1e400}"#).unwrap();
        assert_eq!(parsed.directory.0, b"servers");
        assert_eq!(parsed.id.0, b"demo");
        assert_eq!(parsed.minecraft_version.unwrap().0, b"1.20");
        assert!(
            serde_json::from_str::<ListParams>(r#"{"id":["100.0000000000000001",101,109,111]}"#)
                .is_err()
        );
    }
}

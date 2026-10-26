//! Dedicated read-only RPC boundary. Mutation/archive String APIs are unchanged.
use super::*;
use crate::protocol::RpcError;
use serde::Deserialize;
use serde_json::value::RawValue;

pub(super) struct Bytes(pub(super) Vec<u8>);
impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = <&RawValue>::deserialize(d)?;
        if raw.get().starts_with('"') {
            return serde_json::from_str::<String>(raw.get())
                .map(|v| Self(v.into_bytes()))
                .map_err(serde::de::Error::custom);
        }
        let values: Vec<&RawValue> =
            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
        values
            .into_iter()
            .map(|v| {
                crate::json_number::dynamic_raw(v.get(), 8)
                    .map(|n| n as u8)
                    .map_err(|e| serde::de::Error::custom(e.name()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Self)
    }
}
#[derive(Deserialize)]
struct ListParams {
    directory: Bytes,
    #[serde(default)]
    minecraft_version: Option<Bytes>,
}
#[derive(Deserialize)]
struct GetParams {
    directory: Bytes,
    world: Bytes,
    #[serde(default)]
    minecraft_version: Option<Bytes>,
}

pub(super) fn handle(
    method: &str,
    params: Option<&RawValue>,
) -> std::result::Result<Value, RpcError> {
    let list = method == "worlds.list";
    let (what, code, message) = if list {
        ("worlds list", "WORLD_LIST_FAILED", "Failed to list worlds")
    } else {
        (
            "world lookup",
            "WORLD_GET_FAILED",
            "Failed to inspect the world",
        )
    };
    let params = params.ok_or_else(|| RpcError::new("INVALID_PARAMS", "missing parameters"))?;
    let invalid = || RpcError::new("INVALID_PARAMS", format!("{what} parameters are invalid"));
    // Keep ignored strings well formed without coercing ignored numeric values
    // through binary64. The outer protocol still owns duplicate/lexical errors.
    let json = crate::auth::json::Json::parse(params.get()).map_err(|_| invalid())?;
    if !json.0.starts_with('{') {
        return Err(invalid());
    }
    let (directory, world, version) = if list {
        let p: ListParams = serde_json::from_str(params.get()).map_err(|_| invalid())?;
        (p.directory, None, p.minecraft_version)
    } else {
        let p: GetParams = serde_json::from_str(params.get()).map_err(|_| invalid())?;
        (p.directory, Some(p.world), p.minecraft_version)
    };
    if directory.0.is_empty() || world.as_ref().is_some_and(|v| v.0.is_empty()) {
        return Err(RpcError::new(
            "INVALID_PARAMS",
            if list {
                "worlds listing directory must not be empty"
            } else {
                "world lookup requires directory and world"
            },
        ));
    }
    let result = (|| {
        let path = crate::store::path_from_bytes(&directory.0)?;
        let version = version.as_ref().map(|v| v.0.as_slice());
        match world {
            None => list_read(Path::new(&path), version),
            Some(world) => get_read(Path::new(&path), &world.0, version),
        }
    })();
    result.map_err(|e| rpc::map(e, code, message))
}
pub(super) fn saves_read(path: &Path) -> Result<fs::Dir> {
    fs::Dir::root(path)?.child("saves", false).map_err(absent)
}
fn validate_read_name(name: &[u8]) -> Result<()> {
    if name.is_empty()
        || name.len() > 255
        || name.first() == Some(&b'.')
        || name.last().is_some_and(|b| *b == b' ' || *b == b'.')
        || name.iter().any(|b| *b < 32 || *b == b'/' || *b == b'\\')
    {
        Err(Error::InvalidName)
    } else {
        Ok(())
    }
}
// Listing metadata retains the source NBT byte strings without changing the
// String-oriented operational world APIs.
fn metadata_bytes(doc: Option<&nbt::Document>) -> (Option<&[u8]>, Option<&[u8]>) {
    let data = doc.and_then(|d| d.root.get("Data"));
    fn bytes(tag: Option<&nbt::Tag>) -> Option<&[u8]> {
        match tag {
            Some(nbt::Tag::String(bytes)) => Some(bytes.as_slice()),
            _ => None,
        }
    }
    (
        bytes(data.and_then(|d| d.get("LevelName"))),
        bytes(
            data.and_then(|d| d.get("Version"))
                .and_then(|v| v.get("Name")),
        ),
    )
}
fn metadata_text(bytes: Option<&[u8]>) -> Value {
    match bytes {
        None => Value::Null,
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => json!(text),
            Err(_) => json!(bytes),
        },
    }
}
pub(super) fn world_read(parent: &fs::Dir, name: &[u8], version: Option<&[u8]>) -> Result<Value> {
    world_read_with_metadata(parent, name, version).map(|(value, _)| value)
}
// Keep metadata presence beside the value, never as an additional wire field.
pub(super) fn world_read_with_metadata(
    parent: &fs::Dir,
    name: &[u8],
    version: Option<&[u8]>,
) -> Result<(Value, bool)> {
    validate_read_name(name)?;
    let native_name = crate::store::path_from_bytes(name)?;
    let d = parent.child_alias_native(&native_name).map_err(absent)?;
    let doc = d
        .read_alias("level.dat", 32 * 1024 * 1024)
        .ok()
        .and_then(|b| decode_level(&b).ok());
    let has_metadata = doc
        .as_ref()
        .is_some_and(|d| matches!(d.root.get("Data"), Some(nbt::Tag::Compound(_))));
    let mut obj = serde_json::Map::new();
    obj.insert(
        "name".into(),
        match std::str::from_utf8(name) {
            Ok(text) => json!(text),
            Err(_) => json!(name),
        },
    );
    for (key, value) in level_meta(doc.as_ref()).as_object().unwrap() {
        obj.insert(key.clone(), value.clone());
    }
    obj.insert("size_bytes".into(), json!(d.size()));
    obj.insert("has_icon".into(), json!(d.exists("icon.png")));
    obj.insert("locked".into(), json!(d.locked().unwrap_or(true)));
    let (level_name, world_version) = metadata_bytes(doc.as_ref());
    obj.insert("level_name".into(), metadata_text(level_name));
    obj.insert("version_name".into(), metadata_text(world_version));
    let relation = match (world_version, version) {
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
    Ok((Value::Object(obj), has_metadata))
}
pub(super) fn listed_name_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        name.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut bytes = Vec::new();
        for value in char::decode_utf16(name.encode_wide()) {
            match value {
                Ok(c) => {
                    let mut buf = [0; 4];
                    bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                Err(e) => {
                    let n = e.unpaired_surrogate();
                    bytes.extend_from_slice(&[
                        (0xe0 | (n >> 12)) as u8,
                        (0x80 | ((n >> 6) & 63)) as u8,
                        (0x80 | (n & 63)) as u8,
                    ]);
                }
            }
        }
        bytes
    }
}
fn listed_name_is_temp(name: &[u8]) -> bool {
    [b".jmcl-copy-".as_slice(), b".jmcl-trash-".as_slice()]
        .iter()
        .any(|marker| name.windows(marker.len()).any(|part| part == *marker))
}
pub(super) fn list_read(path: &Path, version: Option<&[u8]>) -> Result<Value> {
    let s = match saves_read(path) {
        Ok(s) => s,
        Err(Error::WorldNotFound) => return Ok(json!({"stage":"worlds.list","worlds":[]})),
        Err(e) => return Err(e),
    };
    cleanup(&s)?;
    let worlds = collect_listed_worlds(s.names()?, |name| world_read(&s, name, version))?;
    Ok(json!({"stage":"worlds.list","worlds":worlds}))
}
fn collect_listed_worlds(
    entries: Vec<(std::ffi::OsString, fs::FileType)>,
    mut inspect: impl FnMut(&[u8]) -> Result<Value>,
) -> Result<Vec<Value>> {
    let mut accepted = vec![];
    // Match source iteration and failure precedence: the limit is checked
    // before inspecting each valid directory, not after sorting candidate names.
    for (native_name, kind) in entries {
        if kind != fs::FileType::Directory {
            continue;
        }
        let name = listed_name_bytes(&native_name);
        if listed_name_is_temp(&name) || validate_read_name(&name).is_err() {
            continue;
        }
        if accepted.len() >= 1024 {
            return Err(Error::TooManyWorlds);
        }
        if let Ok(world) = inspect(&name) {
            accepted.push((name, world));
        }
    }
    accepted.sort_by(|a, b| a.0.cmp(&b.0));
    let worlds: Vec<_> = accepted.into_iter().map(|(_, world)| world).collect();
    Ok(worlds)
}
fn get_read(path: &Path, name: &[u8], version: Option<&[u8]>) -> Result<Value> {
    use base64::Engine;
    let s = saves_read(path)?;
    let world = world_read(&s, name, version)?;
    let native_name = crate::store::path_from_bytes(name)?;
    let icon = match s
        .child_alias_native(&native_name)?
        .read_alias("icon.png", 65536)
    {
        Ok(b) => Some(base64::engine::general_purpose::STANDARD.encode(b)),
        Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => None,
        Err(Error::ArchiveTooLarge) => return Err(Error::IconTooLarge),
        Err(_) => return Err(Error::IconIo),
    };
    Ok(json!({"stage":"worlds.get","world":world,"icon_png_base64":icon}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_bytes_keep_raw_and_quoted_number_distinction() {
        for raw in [
            r#"[65.0000000000000001]"#,
            r#"["+6__5"]"#,
            r#"["0x1.04p6"]"#,
            r#""A""#,
        ] {
            assert_eq!(serde_json::from_str::<Bytes>(raw).unwrap().0, b"A");
        }
        for raw in [
            r#"["65.0000000000000001"]"#,
            "[256]",
            "[true]",
            "[null]",
            "[[65]]",
            "{}",
        ] {
            assert!(serde_json::from_str::<Bytes>(raw).is_err(), "{raw}");
        }
    }
    #[test]
    fn selected_params_ignore_unknown_numbers_not_known_fields() {
        let p: ListParams =
            serde_json::from_str(r#"{"directory":[103],"extra":1e400,"world":false}"#).unwrap();
        assert_eq!(p.directory.0, b"g");
        assert!(
            serde_json::from_str::<GetParams>(r#"{"directory":[103],"world":false,"extra":1e400}"#)
                .is_err()
        );
    }
    #[test]
    fn rpc_params_require_object_not_serde_positional_structs() {
        for method in ["worlds.list", "worlds.get"] {
            let expected = if method == "worlds.list" {
                "worlds list parameters are invalid"
            } else {
                "world lookup parameters are invalid"
            };
            for input in [
                r#"["",null]"#,
                r#"["","Alpha",null]"#,
                "[]",
                "null",
                "true",
                "1",
                r#""text""#,
            ] {
                let raw: &RawValue = serde_json::from_str(input).unwrap();
                let error = handle(method, Some(raw)).unwrap_err();
                assert_eq!(error.code, "INVALID_PARAMS");
                assert_eq!(error.message, expected, "{method}: {input}");
            }
        }
        let raw = serde_json::from_str::<&RawValue>(r#" {"directory":[],"extra":1e400} "#).unwrap();
        assert_eq!(
            handle("worlds.list", Some(raw)).unwrap_err().message,
            "worlds listing directory must not be empty"
        );
    }
    #[test]
    fn listing_limit_checks_valid_entries_before_inspection_and_sorts_after() {
        let names = || {
            (0..1024)
                .rev()
                .map(|i| (format!("World{i:04}").into(), fs::FileType::Directory))
                .collect::<Vec<_>>()
        };
        let mut entries = names();
        entries.extend([
            (".invalid".into(), fs::FileType::Directory),
            ("hidden.jmcl-copy-1".into(), fs::FileType::Directory),
            ("file".into(), fs::FileType::RegularFile),
        ]);
        let mut calls = 0;
        let result = collect_listed_worlds(entries, |name| {
            calls += 1;
            Ok(json!(std::str::from_utf8(name).unwrap()))
        })
        .unwrap();
        assert_eq!(calls, 1024);
        assert_eq!(result.first().unwrap(), "World0000");
        assert_eq!(result.last().unwrap(), "World1023");

        let mut entries = names();
        entries.push(("unreadable".into(), fs::FileType::Directory));
        let mut calls = 0;
        assert!(matches!(
            collect_listed_worlds(entries, |name| {
                calls += 1;
                if name == b"unreadable" {
                    return Err(Error::WorldNotFound);
                }
                Ok(Value::Null)
            }),
            Err(Error::TooManyWorlds)
        ));
        assert_eq!(
            calls, 1024,
            "limit must win before reading the next valid directory"
        );

        let mut entries = vec![("unreadable".into(), fs::FileType::Directory)];
        entries.extend(names());
        let result = collect_listed_worlds(entries, |name| {
            if name == b"unreadable" {
                Err(Error::WorldNotFound)
            } else {
                Ok(Value::Null)
            }
        })
        .unwrap();
        assert_eq!(
            result.len(),
            1024,
            "failed world inspection must not count as an accepted world"
        );
    }
    #[test]
    fn listing_temp_markers_are_byte_predicates_only() {
        for name in [b"Alpha.jmcl-copy-1".as_slice(), b"\xff.jmcl-trash-2"] {
            assert!(listed_name_is_temp(name));
        }
        for name in [b"Alpha".as_slice(), b"\xffAlpha", b".jmcl-cop"] {
            assert!(!listed_name_is_temp(name));
        }
    }
    #[test]
    #[cfg(windows)]
    fn listing_names_preserve_wtf8_and_roundtrip_native_units() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        for units in [
            vec![0x41],
            vec![0xd800],
            vec![0xdc00],
            vec![0xd83d, 0xde00],
            vec![0xd800, 0xd800],
        ] {
            let name = std::ffi::OsString::from_wide(&units);
            let bytes = listed_name_bytes(&name);
            assert_eq!(
                crate::store::path_from_bytes(&bytes)
                    .unwrap()
                    .encode_wide()
                    .collect::<Vec<_>>(),
                units
            );
        }
        assert_eq!(
            listed_name_bytes(&std::ffi::OsString::from_wide(&[0xd800])),
            [0xed, 0xa0, 0x80]
        );
    }
    #[test]
    fn byte_name_validation_retains_boundary_and_semantic_rules() {
        for name in [b"Alpha".as_slice(), &[255], &[b'A'; 255]] {
            assert!(validate_read_name(name).is_ok());
        }
        for name in [
            b"".as_slice(),
            b"..",
            b"A/",
            b"A\\",
            b"A ",
            &[0],
            &[b'A'; 256],
        ] {
            assert!(matches!(validate_read_name(name), Err(Error::InvalidName)));
        }
    }
}

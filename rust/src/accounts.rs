//! Public Microsoft account metadata, compatible with the Zig account registry.
//! Secrets are deliberately absent from the disk model. Windows handle-relative
//! filesystem parity is a separate gate; unsupported backends fail closed.
use crate::protocol::RpcError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};

const MAX_MANIFEST_BYTES: u64 = 16 * 1024;
const MAX_ACCOUNTS: usize = 4096;
const DEFAULT_ROOT: &str = "accounts";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    #[serde(default = "default_schema", deserialize_with = "deserialize_param_u32")]
    pub schema_version: u32,
    pub id: String,
    pub player_name: String,
    pub client_id: String,
    pub xuid: Option<String>,
}

// A typed disk visitor rejects duplicate known fields while ignoring unknown
// ones, matching Zig's parseFromSlice(DiskAccount, ignore_unknown_fields=true).
#[derive(Deserialize)]
struct DiskAccount {
    #[serde(deserialize_with = "deserialize_u32")]
    schema_version: u32,
    #[serde(deserialize_with = "deserialize_disk_string")]
    id: String,
    #[serde(deserialize_with = "deserialize_disk_string")]
    player_name: String,
    #[serde(deserialize_with = "deserialize_disk_string")]
    client_id: String,
    #[serde(default, deserialize_with = "deserialize_disk_optional_string")]
    xuid: Option<String>,
}
// Zig's directly parsed []const u8 accepts strings or arrays of typed u8.
// Keep this disk-only: RPC parameters use a different dynamic-number grammar.
fn disk_string(raw: &RawValue) -> Result<String, &'static str> {
    if raw.get().trim_start().starts_with('[') {
        let elements: Vec<&RawValue> =
            serde_json::from_str(raw.get()).map_err(|_| "invalid byte array")?;
        let bytes = elements
            .into_iter()
            .map(|element| {
                crate::json_number::typed_raw(element.get(), 8)
                    .map(|n| n as u8)
                    .map_err(|_| "invalid u8")
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Every valid account field is ASCII under validate(). Non-UTF8 byte
        // arrays cannot form valid account metadata and stay rejected.
        String::from_utf8(bytes).map_err(|_| "invalid account string")
    } else {
        serde_json::from_str(raw.get()).map_err(|_| "invalid account string")
    }
}
fn deserialize_disk_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    disk_string(<&RawValue>::deserialize(d)?).map_err(serde::de::Error::custom)
}
fn deserialize_disk_optional_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    if raw.get().trim() == "null" {
        Ok(None)
    } else {
        disk_string(raw).map(Some).map_err(serde::de::Error::custom)
    }
}

fn deserialize_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), 32)
        .map(|n| n as u32)
        .map_err(|_| serde::de::Error::custom("invalid u32"))
}
// RPC parameters first pass through Zig's dynamic Value representation, whereas
// the on-disk document is parsed directly into its typed schema. Preserve both.
fn deserialize_param_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    crate::json_number::dynamic_raw(raw.get(), 32)
        .map(|n| n as u32)
        .map_err(|_| serde::de::Error::custom("invalid u32"))
}

#[derive(Debug)]
enum Error {
    InvalidMetadata,
    InvalidManifest,
    NotFound,
    Limit,
    Io,
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound
        } else {
            Self::Io
        }
    }
}
#[cfg(unix)]
impl From<rustix::io::Errno> for Error {
    fn from(error: rustix::io::Errno) -> Self {
        std::io::Error::from(error).into()
    }
}
impl Error {
    fn rpc(self) -> RpcError {
        match self {
            Self::InvalidMetadata => RpcError::new("INVALID_PARAMS", "Account metadata is invalid"),
            Self::InvalidManifest => {
                RpcError::new("ACCOUNT_STORE_ERROR", "Stored account metadata is invalid")
            }
            Self::NotFound => RpcError::new("ACCOUNT_NOT_FOUND", "Account metadata was not found"),
            Self::Limit => RpcError::new(
                "ACCOUNT_LIMIT_REACHED",
                "Account registry has too many entries",
            ),
            Self::Io => RpcError::new("ACCOUNT_STORE_ERROR", "Account registry operation failed"),
        }
    }
}
fn params_error(message: &str) -> RpcError {
    RpcError::new("INVALID_PARAMS", message)
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_root(root: &str) -> bool {
    !root.is_empty()
        && root.len() <= 64
        && root
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn validate(account: &Account) -> Result<(), Error> {
    if !valid_id(&account.id)
        || account.schema_version != 1
        || account.player_name.is_empty()
        || account.player_name.len() > 16
        || !account
            .player_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || account.client_id.len() != 36
        || !account.client_id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        || account
            .xuid
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 32 || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(Error::InvalidMetadata);
    }
    Ok(())
}
struct ObjectOnly<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
            type Value = ObjectOnly<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(ObjectOnly)
            }
        }
        deserializer.deserialize_map(Visitor(std::marker::PhantomData))
    }
}
// Raw parameter byte strings must survive parsing even when non-UTF8: Zig
// validates them only after empty checks and root opening. Keep disk decoding
// and public serialized Account strings separate from this dynamic boundary.
#[derive(Debug)]
struct ParamBytes(Vec<u8>);
impl ParamBytes {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn text(&self) -> Result<&str, Error> {
        std::str::from_utf8(&self.0).map_err(|_| Error::InvalidMetadata)
    }
    fn into_text(self) -> Result<String, Error> {
        String::from_utf8(self.0).map_err(|_| Error::InvalidMetadata)
    }
}
impl<'de> Deserialize<'de> for ParamBytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <&RawValue>::deserialize(d)?;
        if raw.get().trim_start().starts_with('[') {
            let elements: Vec<&RawValue> =
                serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
            elements
                .into_iter()
                .map(|element| {
                    crate::json_number::dynamic_raw(element.get(), 8)
                        .map(|n| n as u8)
                        .map_err(|_| serde::de::Error::custom("invalid parameter byte"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Self)
        } else {
            serde_json::from_str::<String>(raw.get())
                .map(|s| Self(s.into_bytes()))
                .map_err(serde::de::Error::custom)
        }
    }
}
#[derive(Deserialize)]
struct ParamAccount {
    #[serde(default = "default_schema", deserialize_with = "deserialize_param_u32")]
    schema_version: u32,
    id: ParamBytes,
    player_name: ParamBytes,
    client_id: ParamBytes,
    xuid: Option<ParamBytes>,
}
impl ParamAccount {
    fn into_account(self) -> Result<Account, Error> {
        Ok(Account {
            schema_version: self.schema_version,
            id: self.id.into_text()?,
            player_name: self.player_name.into_text()?,
            client_id: self.client_id.into_text()?,
            xuid: self.xuid.map(ParamBytes::into_text).transpose()?,
        })
    }
}
fn deserialize_account<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ParamAccount, D::Error> {
    ObjectOnly::<ParamAccount>::deserialize(d).map(|v| v.0)
}

fn default_schema() -> u32 {
    1
}
fn default_root() -> ParamBytes {
    ParamBytes(DEFAULT_ROOT.as_bytes().to_vec())
}
#[derive(Deserialize)]
struct DirectoryParams {
    #[serde(default = "default_root")]
    directory: ParamBytes,
}
#[derive(Deserialize)]
struct SaveParams {
    #[serde(default = "default_root")]
    directory: ParamBytes,
    #[serde(deserialize_with = "deserialize_account")]
    account: ParamAccount,
}
#[derive(Deserialize)]
struct TargetParams {
    #[serde(default = "default_root")]
    directory: ParamBytes,
    id: ParamBytes,
}

/// Only account.microsoft.* methods are claimed. All returned Values are result
/// payloads; the protocol layer owns the terminal envelope and request ID.
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    if !matches!(
        method,
        "account.microsoft.save"
            | "account.microsoft.list"
            | "account.microsoft.get"
            | "account.microsoft.delete"
    ) {
        return None;
    }
    Some(handle_inner(method, params))
}
fn handle_inner(method: &str, params: Option<&RawValue>) -> Result<Value, RpcError> {
    // Typed parsing ignores unknown fields without allocating their values.
    // Accidental caller-supplied tokens never enter the public Account model.
    let params = params.filter(|p| p.get().trim() != "null");
    match method {
        "account.microsoft.save" => {
            let raw = params.ok_or_else(|| {
                params_error("account.microsoft.save requires public account metadata")
            })?;
            let params: SaveParams = serde_json::from_str::<ObjectOnly<SaveParams>>(raw.get())
                .map(|v| v.0)
                .map_err(|_| params_error("Account metadata parameters are invalid"))?;
            if params.directory.is_empty() {
                return Err(params_error("Account directory must not be empty"));
            }
            let store =
                Store::open(params.directory.text().map_err(Error::rpc)?).map_err(Error::rpc)?;
            let account = params.account.into_account().map_err(Error::rpc)?;
            store.save(&account).map_err(Error::rpc)?;
            Ok(json!({"stage":"save","account":account}))
        }
        "account.microsoft.list" => {
            let params: DirectoryParams = match params {
                Some(raw) => serde_json::from_str::<ObjectOnly<DirectoryParams>>(raw.get())
                    .map(|v| v.0)
                    .map_err(|_| params_error("Account list parameters are invalid"))?,
                None => DirectoryParams {
                    directory: default_root(),
                },
            };
            if params.directory.is_empty() {
                return Err(params_error("Account directory must not be empty"));
            }
            let store =
                Store::open(params.directory.text().map_err(Error::rpc)?).map_err(Error::rpc)?;
            Ok(json!({"stage":"account_list","accounts":store.list().map_err(Error::rpc)?}))
        }
        _ => {
            let raw = params.ok_or_else(|| params_error("Account operation requires an ID"))?;
            let params: TargetParams = serde_json::from_str::<ObjectOnly<TargetParams>>(raw.get())
                .map(|v| v.0)
                .map_err(|_| params_error("Account target parameters are invalid"))?;
            if params.directory.is_empty() || params.id.is_empty() {
                return Err(params_error("Account directory and ID must not be empty"));
            }
            let store =
                Store::open(params.directory.text().map_err(Error::rpc)?).map_err(Error::rpc)?;
            let id = params.id.text().map_err(Error::rpc)?;
            if method == "account.microsoft.get" {
                Ok(json!({"stage":"account_get","account":store.load(id).map_err(Error::rpc)?}))
            } else {
                store.delete(id).map_err(Error::rpc)?;
                Ok(json!({"stage":"account_delete","id":id,"deleted":true}))
            }
        }
    }
}

#[cfg(unix)]
struct Store {
    root: std::fs::File,
}
#[cfg(unix)]
impl Store {
    fn open(name: &str) -> Result<Self, Error> {
        Self::open_at(&std::fs::File::open(".")?, name)
    }
    fn open_at(parent: &std::fs::File, name: &str) -> Result<Self, Error> {
        use rustix::fs::{Mode, OFlags, mkdirat, openat};
        if !valid_root(name) {
            return Err(Error::InvalidMetadata);
        }
        match mkdirat(parent, name, Mode::from_bits_truncate(0o755)) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => (),
            Err(e) => return Err(e.into()),
        }
        let root = openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Self { root: root.into() })
    }
    fn file(&self, name: &str, flags: rustix::fs::OFlags) -> Result<std::fs::File, Error> {
        use rustix::fs::{Mode, OFlags, openat};
        let file: std::fs::File = openat(
            &self.root,
            name,
            flags | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o644),
        )?
        .into();
        if !file.metadata()?.is_file() {
            return Err(Error::Io);
        }
        Ok(file)
    }
    fn names(&self) -> Result<Vec<String>, Error> {
        use rustix::fs::{Dir, FileType};
        let mut names = Vec::new();
        for entry in Dir::read_from(&self.root)? {
            let entry = entry?;
            if !matches!(entry.file_type(), FileType::RegularFile | FileType::Unknown) {
                continue;
            }
            let Ok(name) = entry.file_name().to_str() else {
                continue;
            };
            if name.len() == 37 && name.ends_with(".json") && valid_id(&name[..32]) {
                names.push(name[..32].to_owned());
            }
        }
        Ok(names)
    }
    fn load(&self, id: &str) -> Result<Account, Error> {
        use std::io::Read;
        if !valid_id(id) {
            return Err(Error::InvalidMetadata);
        }
        let mut file = self.file(&format!("{id}.json"), rustix::fs::OFlags::RDONLY)?;
        let size = file.metadata()?.len();
        if size > MAX_MANIFEST_BYTES {
            return Err(Error::InvalidManifest);
        }
        let mut bytes = vec![0; size as usize];
        file.read_exact(&mut bytes).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                Error::InvalidManifest
            } else {
                error.into()
            }
        })?;
        let disk: DiskAccount = serde_json::from_slice::<ObjectOnly<DiskAccount>>(&bytes)
            .map(|v| v.0)
            .map_err(|_| Error::InvalidManifest)?;
        let account = Account {
            schema_version: disk.schema_version,
            id: disk.id,
            player_name: disk.player_name,
            client_id: disk.client_id,
            xuid: disk.xuid,
        };
        validate(&account).map_err(|_| Error::InvalidManifest)?;
        if account.id != id {
            return Err(Error::InvalidManifest);
        }
        Ok(account)
    }
    fn save(&self, account: &Account) -> Result<(), Error> {
        use rustix::fs::{AtFlags, OFlags, renameat, unlinkat};
        use std::io::Write;
        validate(account)?;
        let lock = self.file(".registry.lock", OFlags::RDWR | OFlags::CREATE)?;
        lock.lock()?;
        let filename = format!("{}.json", account.id);
        match self.file(&filename, OFlags::RDONLY) {
            Ok(_) => (),
            Err(Error::NotFound) => {
                if self.names()?.len() >= MAX_ACCOUNTS {
                    return Err(Error::Limit);
                }
            }
            Err(e) => return Err(e),
        }
        // Exclusive create, followed by descriptor-relative rename. A failed
        // attempt removes only the temporary file created by this invocation.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let (temp_name, mut temp) = loop {
            let name = format!(
                ".account-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            match self.file(&name, OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL) {
                Ok(file) => break (name, file),
                Err(Error::Io) => {
                    // distinguish collision from other failures without following links
                    match rustix::fs::statat(&self.root, &name, AtFlags::SYMLINK_NOFOLLOW) {
                        Ok(_) => continue,
                        Err(_) => return Err(Error::Io),
                    }
                }
                Err(e) => return Err(e),
            }
        };
        let result = (|| {
            serde_json::to_writer(&mut temp, account).map_err(|_| Error::Io)?;
            temp.write_all(b"\n")?;
            temp.flush()?;
            temp.sync_all()?;
            renameat(&self.root, &temp_name, &self.root, &filename)?;
            Ok(())
        })();
        let _ = unlinkat(&self.root, &temp_name, AtFlags::empty());
        result
    }
    fn list(&self) -> Result<Vec<Account>, Error> {
        let mut result = Vec::new();
        for id in self.names()? {
            if result.len() == MAX_ACCOUNTS {
                return Err(Error::Limit);
            }
            match self.load(&id) {
                Ok(account) => result.push(account),
                Err(Error::InvalidManifest) => (),
                Err(e) => return Err(e),
            }
        }
        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    }
    fn delete(&self, id: &str) -> Result<(), Error> {
        self.load(id)?;
        rustix::fs::unlinkat(
            &self.root,
            format!("{id}.json"),
            rustix::fs::AtFlags::empty(),
        )?;
        Ok(())
    }
}
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::Store;

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    fn sample() -> Account {
        Account {
            schema_version: 1,
            id: "0123456789abcdef0123456789abcdef".into(),
            player_name: "Player_1".into(),
            client_id: "00001111-aaaa-2222-bbbb-3333cccc4444".into(),
            xuid: Some("123".into()),
        }
    }
    fn store(tmp: &tempfile::TempDir) -> Store {
        #[cfg(unix)]
        let parent = fs::File::open(tmp.path()).unwrap();
        #[cfg(windows)]
        let parent = crate::store::windows::walk(tmp.path(), false).unwrap();
        Store::open_at(&parent, "accounts").unwrap()
    }
    #[test]
    fn save_load_list_delete_public_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(&tmp);
        let mut a = sample();
        store.save(&a).unwrap();
        assert_eq!(store.load(&a.id).unwrap(), a);
        a.player_name = "Updated".into();
        a.xuid = None;
        store.save(&a).unwrap();
        assert_eq!(store.list().unwrap(), vec![a.clone()]);
        let bytes = fs::read(tmp.path().join("accounts").join(format!("{}.json", a.id))).unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            serde_json::to_value(&a).unwrap()
        );
        store.delete(&a.id).unwrap();
        assert!(matches!(store.load(&a.id), Err(Error::NotFound)));
    }
    #[test]
    fn invalid_and_corrupt_accounts_are_not_mutated() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(&tmp);
        let a = sample();
        let path = tmp.path().join("accounts").join(format!("{}.json", a.id));
        for bytes in [b"{}".as_slice(), b"{not json}", b"{\"schema_version\":1}"] {
            fs::write(&path, bytes).unwrap();
            assert!(matches!(store.load(&a.id), Err(Error::InvalidManifest)));
            assert!(store.list().unwrap().is_empty());
            assert!(matches!(store.delete(&a.id), Err(Error::InvalidManifest)));
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, vec![b' '; 16 * 1024 + 1]).unwrap();
        assert!(matches!(store.load(&a.id), Err(Error::InvalidManifest)));
    }
    #[cfg(unix)]
    #[test]
    fn nofollow_root_manifest_and_lock() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("elsewhere")).unwrap();
        symlink("elsewhere", tmp.path().join("rootlink")).unwrap();
        assert!(Store::open_at(&fs::File::open(tmp.path()).unwrap(), "rootlink").is_err());
        let store = store(&tmp);
        let a = sample();
        fs::write(tmp.path().join("outside"), b"keep").unwrap();
        symlink(
            "../outside",
            tmp.path().join("accounts").join(format!("{}.json", a.id)),
        )
        .unwrap();
        assert!(store.load(&a.id).is_err());
        assert!(store.save(&a).is_err());
        assert_eq!(fs::read(tmp.path().join("outside")).unwrap(), b"keep");
        fs::remove_file(tmp.path().join("accounts").join(format!("{}.json", a.id))).unwrap();
        fs::remove_file(tmp.path().join("accounts/.registry.lock")).unwrap();
        symlink("../outside", tmp.path().join("accounts/.registry.lock")).unwrap();
        assert!(store.save(&a).is_err());
    }
    #[test]
    fn disk_known_duplicates_rejected_unknown_fields_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(&tmp);
        let a = sample();
        let path = tmp.path().join("accounts").join(format!("{}.json", a.id));
        let base = serde_json::to_string(&a).unwrap();
        fs::write(
            &path,
            base.replacen("{", "{\"id\":\"0123456789abcdef0123456789abcdef\",", 1),
        )
        .unwrap();
        assert!(matches!(store.load(&a.id), Err(Error::InvalidManifest)));
        fs::write(
            &path,
            base.replacen("{", "{\"unknown\":1,\"unknown\":2,", 1),
        )
        .unwrap();
        assert_eq!(store.load(&a.id).unwrap(), a);
    }
    #[test]
    fn matching_directory_is_not_an_account_or_replaced_by_save() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(&tmp);
        let account = sample();
        let path = tmp
            .path()
            .join("accounts")
            .join(format!("{}.json", account.id));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), b"untouched").unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(store.save(&account).is_err());
        assert!(store.load(&account.id).is_err());
        assert!(store.delete(&account.id).is_err());
        assert_eq!(fs::read(path.join("keep")).unwrap(), b"untouched");
    }
    #[test]
    fn validation_and_numeric_schema_coercion() {
        assert_eq!(
            deserialize_param_u32(&mut serde_json::Deserializer::from_str(
                "1.0000000000000001"
            ))
            .unwrap(),
            1
        );
        assert!(
            deserialize_param_u32(&mut serde_json::Deserializer::from_str(
                "\"1.0000000000000001\""
            ))
            .is_err()
        );
        for text in [
            "1",
            "1.0",
            "\"1\"",
            "\"1.0\"",
            "\"1e0\"",
            "\"+1\"",
            "\"01\"",
            "\"0_1\"",
            "\"0x1.0p0\"",
            "1.0000000000000000000000000000000000000001",
        ] {
            assert_eq!(
                deserialize_u32(&mut serde_json::Deserializer::from_str(text)).unwrap(),
                1
            );
        }
        for text in [
            "null",
            "true",
            "-1",
            "1.5",
            "\" 1\"",
            "\"0x1\"",
            "1.0000000000000001",
            "\"1.0000000000000001\"",
            "4294967296",
            "[]",
            "{}",
        ] {
            assert!(deserialize_u32(&mut serde_json::Deserializer::from_str(text)).is_err());
        }
        for root in ["", ".", "..", "/tmp/accounts", "a/b", "a\\b", "a:b", "☃"] {
            assert!(!valid_root(root));
        }
        let mut a = sample();
        a.client_id = "secret-token".into();
        assert!(validate(&a).is_err());
    }
    #[test]
    fn registry_capacity_counts_bad_matching_manifests_but_allows_update() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(&tmp);
        let a = sample();
        store.save(&a).unwrap();
        for i in 0..MAX_ACCOUNTS - 1 {
            fs::write(
                tmp.path().join("accounts").join(format!("{i:032x}.json")),
                b"{}",
            )
            .unwrap();
        }
        store.save(&a).unwrap();
        let mut other = a;
        other.id = "ffffffffffffffffffffffffffffffff".into();
        assert!(matches!(store.save(&other), Err(Error::Limit)));
    }
}

#[cfg(test)]
mod disk_byte_tests {
    use super::*;
    #[test]
    fn disk_bytes_keep_typed_grammar_and_parameter_scope_separate() {
        for raw in [
            r#""Player_1""#,
            "[80,108,97,121,101,114,95,49]",
            r#"["80",108.0,97,121,101,114,95,49]"#,
            "[80.00000000000000000000000000000000000001,108,97,121,101,114,95,49]",
        ] {
            assert_eq!(
                disk_string(RawValue::from_string(raw.into()).unwrap().as_ref()).unwrap(),
                "Player_1"
            );
        }
        for raw in [
            "[80.0000000000000001]",
            "[256]",
            "[-1]",
            "[true]",
            "[null]",
            "[{}]",
            "[[]]",
            "[255]",
            "null",
            "{}",
        ] {
            assert!(
                disk_string(RawValue::from_string(raw.into()).unwrap().as_ref()).is_err(),
                "{raw}"
            );
        }
        // Parameter decoding must not acquire the disk's typed array grammar.
        let params = r#"{"id":"0123456789abcdef0123456789abcdef","player_name":[80],"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444"}"#;
        assert!(serde_json::from_str::<Account>(params).is_err());
    }
}

#[cfg(test)]
mod param_byte_tests {
    use super::*;
    #[test]
    fn parameter_bytes_preserve_dynamic_rounding_and_semantic_boundaries() {
        let rounded: ParamBytes = serde_json::from_str("[48.0000000000000001]").unwrap();
        assert_eq!(rounded.0, b"0");
        assert!(serde_json::from_str::<ParamBytes>(r#"["48.0000000000000001"]"#).is_err());
        assert!(
            disk_string(
                RawValue::from_string("[48.0000000000000001]".into())
                    .unwrap()
                    .as_ref()
            )
            .is_err()
        );
        let opaque: ParamBytes = serde_json::from_str("[255,0]").unwrap();
        assert_eq!(opaque.0, [255, 0]);
        assert!(!opaque.is_empty());
        assert!(opaque.text().is_err());
        assert!(serde_json::from_str::<ParamBytes>("[]").unwrap().is_empty());
        for raw in [
            "[256]", "[-1]", "[null]", "[true]", "[{}]", "[[]]", "null", "{}",
        ] {
            assert!(serde_json::from_str::<ParamBytes>(raw).is_err(), "{raw}");
        }
    }
}

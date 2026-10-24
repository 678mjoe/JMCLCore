//! The same public registry using the shared Windows descriptor-relative layer.
//! No ambient leaf access: all manifest operations are anchored to `root`.
use super::*;
use crate::store::windows as fs;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};

pub(super) struct Store {
    root: File,
}
impl Store {
    pub(super) fn open(name: &str) -> Result<Self, Error> {
        if !valid_root(name) {
            return Err(Error::InvalidMetadata);
        }
        Self::open_at(&fs::walk(Path::new("."), false)?, name)
    }
    pub(super) fn open_at(parent: &File, name: &str) -> Result<Self, Error> {
        if !valid_root(name) {
            return Err(Error::InvalidMetadata);
        }
        Ok(Self {
            root: fs::child(parent, OsStr::new(name), true, true, false)?,
        })
    }
    fn names(&self) -> Result<Vec<String>, Error> {
        let mut result = Vec::new();
        for (name, attrs) in fs::entries(&self.root)? {
            if attrs & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
                continue;
            }
            let Some(name) = name.to_str() else { continue };
            if name.len() == 37 && name.ends_with(".json") && valid_id(&name[..32]) {
                result.push(name[..32].to_owned());
            }
        }
        Ok(result)
    }
    pub(super) fn load(&self, id: &str) -> Result<Account, Error> {
        if !valid_id(id) {
            return Err(Error::InvalidMetadata);
        }
        let mut file = fs::read_at(&self.root, OsStr::new(&format!("{id}.json")))?;
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
    pub(super) fn save(&self, account: &Account) -> Result<(), Error> {
        validate(account)?;
        let lock = fs::child(&self.root, OsStr::new(".registry.lock"), false, true, true)?;
        lock.lock()?;
        let name = format!("{}.json", account.id);
        match fs::read_at(&self.root, OsStr::new(&name)) {
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if self.names()?.len() >= MAX_ACCOUNTS {
                    return Err(Error::Limit);
                }
            }
            Err(e) => return Err(e.into()),
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut temp = loop {
            let name = format!(
                ".account-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            match fs::create_file(&self.root, OsStr::new(&name)) {
                Ok(file) => break file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        };
        let result = (|| {
            serde_json::to_writer(&mut temp, account).map_err(|_| Error::Io)?;
            temp.write_all(b"\n")?;
            temp.flush()?;
            temp.sync_all()?;
            fs::named_op(&temp, &self.root, OsStr::new(&name), true)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::unlink(&temp);
        }
        result
    }
    pub(super) fn list(&self) -> Result<Vec<Account>, Error> {
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
    pub(super) fn delete(&self, id: &str) -> Result<(), Error> {
        self.load(id)?;
        let file = fs::file_at(&self.root, OsStr::new(&format!("{id}.json")))?;
        fs::unlink(&file)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn junction_root_and_manifest_are_not_followed() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep"), b"untouched").unwrap();
        let root_link = temp.path().join("junction");
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&root_link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let parent = fs::walk(temp.path(), false).unwrap();
        assert!(Store::open_at(&parent, "junction").is_err());
        let store = Store::open_at(&parent, "accounts").unwrap();
        let account = Account {
            schema_version: 1,
            id: "0123456789abcdef0123456789abcdef".into(),
            player_name: "Player".into(),
            client_id: "00001111-aaaa-2222-bbbb-3333cccc4444".into(),
            xuid: None,
        };
        let manifest = temp
            .path()
            .join("accounts")
            .join(format!("{}.json", account.id));
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&manifest)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(store.list().unwrap().is_empty());
        assert!(store.load(&account.id).is_err());
        assert!(store.save(&account).is_err());
        assert!(store.delete(&account.id).is_err());
        assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"untouched");
        std::fs::remove_dir(manifest).unwrap();
        std::fs::remove_dir(root_link).unwrap();
    }
}

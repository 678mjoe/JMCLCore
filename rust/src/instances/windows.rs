//! Windows instance adapter using shared handle-relative NT filesystem primitives.
use super::{
    InstallationIdentity, MAX_MANIFEST_BYTES, Manifest, ReadManifest, installed_bytes, validate_id,
};
use crate::store::windows as fs;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
pub struct Root {
    dir: File,
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s.to_owned())
}
fn read(dir: &File, name: &str, max: usize) -> io::Result<Vec<u8>> {
    let file = if name == "instance.json" {
        fs::read_follow_at(dir, name.as_ref())?
    } else {
        fs::read_at(dir, name.as_ref())?
    };
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(invalid("TooLarge"));
    }
    Ok(bytes)
}
fn atomic_write(dir: &File, name: &str, data: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp = format!(
        ".jmcl-manifest-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = fs::create_file(dir, temp.as_ref())?;
    let result = (|| {
        file.write_all(data)?;
        file.sync_all()?;
        fs::named_op(&file, dir, name.as_ref(), true)
    })();
    if result.is_err() {
        let _ = fs::unlink(&file);
    }
    result
}
impl Root {
    /// Read-only existence preflight matches the legacy openDir alias policy.
    pub fn contains_directory(&self, id: &str) -> io::Result<bool> {
        validate_id(id).map_err(invalid)?;
        match fs::read_dir_follow_at(&self.dir, id.as_ref()) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
    /// Newly-created instance descendants are resolved from the pinned root.
    pub fn game_root(&self, id: &str) -> io::Result<crate::store::ArtifactRoot> {
        validate_id(id).map_err(invalid)?;
        let child = fs::child(&self.dir, id.as_ref(), true, false, false)?;
        let game = fs::child(&child, ".minecraft".as_ref(), true, false, false)?;
        crate::store::ArtifactRoot::from_directory(game)
    }

    pub fn open(path: &str) -> io::Result<Self> {
        Self::open_native(path.as_ref())
    }
    pub fn open_native(path: &std::ffi::OsStr) -> io::Result<Self> {
        if path.is_empty() {
            return Err(invalid("EmptyInstancesRoot"));
        }
        // Match createDirPathOpen: an existing final file must reach the
        // directory-open operation (NotDir), not stop at mkdir AlreadyExists.
        match std::fs::create_dir_all(std::path::Path::new(path)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        Ok(Self {
            dir: fs::walk(Path::new(path), false)?,
        })
    }
    pub fn create(&self, m: &Manifest) -> io::Result<()> {
        m.validate().map_err(invalid)?;
        self.create_with(&m.id, || m.encode().map_err(invalid))
    }
    // Callers validate the manifest/ID before entering this anchored writer.
    // Encoding remains after directory creation, preserving cleanup on failure.
    pub(super) fn create_with(
        &self,
        id: &str,
        encode: impl FnOnce() -> io::Result<Vec<u8>>,
    ) -> io::Result<()> {
        let child = fs::create_dir(&self.dir, id.as_ref())?;
        let result = (|| {
            let game = fs::create_dir(&child, ".minecraft".as_ref())?;
            drop(game);
            atomic_write(&child, "instance.json", &encode()?)
        })();
        drop(child);
        if result.is_err() {
            let _ = fs::remove_tree(&self.dir, id.as_ref());
        }
        result
    }
    pub fn load(&self, id: &str) -> io::Result<Manifest> {
        self.load_with(id, Manifest::parse)
    }
    pub(crate) fn load_presentation_version(&self, id: &str) -> io::Result<Vec<u8>> {
        self.load_with(id, super::byte_read::version_bytes)
    }
    pub(super) fn load_bytes(&self, id: &str) -> io::Result<super::byte_read::Manifest> {
        self.load_with(id, super::byte_read::Manifest::parse)
    }
    pub(super) fn list_bytes(&self) -> io::Result<Vec<super::byte_read::Manifest>> {
        self.list_with(Self::load_bytes, super::byte_read::Manifest::id)
    }
    pub(super) fn installed_byte_view(&self, m: &super::byte_read::Manifest) -> io::Result<bool> {
        self.installed_with(m.id(), |bytes| m.matches_receipt(bytes))
    }
    pub fn load_read(&self, id: &str) -> io::Result<ReadManifest> {
        self.load_with(id, ReadManifest::parse)
    }
    fn load_with<T>(
        &self,
        id: &str,
        parse: fn(&[u8], &str) -> Result<T, &'static str>,
    ) -> io::Result<T> {
        validate_id(id).map_err(invalid)?;
        let child = fs::child(&self.dir, id.as_ref(), true, false, false)?;
        parse(
            &read(&child, "instance.json", MAX_MANIFEST_BYTES).map_err(|e| {
                if e.to_string() == "TooLarge" {
                    invalid("InstanceManifestTooLarge")
                } else {
                    e
                }
            })?,
            id,
        )
        .map_err(invalid)
    }
    #[cfg(test)]
    pub fn list(&self) -> io::Result<Vec<Manifest>> {
        self.list_with(Self::load, |m| &m.id)
    }
    #[allow(dead_code)] // Preserved String-based API; presentation uses list_bytes.
    pub fn list_read(&self) -> io::Result<Vec<ReadManifest>> {
        self.list_with(Self::load_read, |m| &m.id)
    }
    fn list_with<T>(
        &self,
        load: fn(&Self, &str) -> io::Result<T>,
        id_of: fn(&T) -> &str,
    ) -> io::Result<Vec<T>> {
        let mut values = Vec::new();
        for name in fs::names(&self.dir)? {
            let Some(id) = name.to_str() else { continue };
            if validate_id(id).is_err() {
                continue;
            }
            let Ok(child) = fs::child(&self.dir, &name, true, false, false) else {
                continue;
            };
            drop(child);
            if values.len() == 4096 {
                return Err(invalid("TooManyInstances"));
            }
            if let Ok(m) = load(self, id) {
                values.push(m)
            }
        }
        values.sort_by(|a, b| id_of(a).cmp(id_of(b)));
        Ok(values)
    }
    pub fn delete(&self, id: &str) -> io::Result<()> {
        self.load_bytes(id)?;
        fs::remove_tree(&self.dir, id.as_ref())
    }
    #[cfg(test)]
    pub fn installed(&self, m: &Manifest) -> io::Result<bool> {
        self.installed_identity(m.identity())
    }
    #[allow(dead_code)] // Preserved String-based API and test controls.
    pub fn installed_identity(&self, m: InstallationIdentity<'_>) -> io::Result<bool> {
        self.installed_with(m.id, |bytes| installed_bytes(bytes, m))
    }
    fn installed_with(&self, id: &str, matches: impl FnOnce(&[u8]) -> bool) -> io::Result<bool> {
        let child = fs::child(&self.dir, id.as_ref(), true, false, false)?;
        let game = match fs::child(&child, ".minecraft".as_ref(), true, false, false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        let meta = match fs::child(&game, ".jmcl".as_ref(), true, false, false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match read(&meta, "install.json", 16 * 1024) {
            Ok(b) => Ok(matches(&b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound || e.to_string() == "TooLarge" => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_file_root_reports_not_dir_without_modification() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("blocked");
        std::fs::write(&path, b"not a directory").unwrap();
        let error = match Root::open(path.to_str().unwrap()) {
            Ok(_) => panic!("file must not open as an instances directory"),
            Err(error) => error,
        };
        assert_eq!(super::super::cli::zig_error(&error), "NotDir");
        assert_eq!(std::fs::read(&path).unwrap(), b"not a directory");
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 1);
    }
    #[test]
    fn lifecycle_uses_existing_manifest_schema() {
        let t = tempfile::tempdir().unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        let m = Manifest::parse(br#"{"id":"a","name":"A","version_id":"1.21"}"#, "a").unwrap();
        root.create(&m).unwrap();
        assert_eq!(root.load("a").unwrap(), m);
        assert!(root.create(&m).is_err());
        assert_eq!(root.list().unwrap(), vec![m.clone()]);
        assert!(!root.installed(&m).unwrap());
        root.delete("a").unwrap();
        assert!(root.list().unwrap().is_empty());
    }
}

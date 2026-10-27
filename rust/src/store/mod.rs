//! Legacy-compatible immutable artifact store. Mutable worlds/configuration must
//! never be passed to this API. Unix uses directory-relative, no-follow opens.
//! Windows uses a handle-relative NT backend with explicit reparse rejection.
//!
//! Lock ownership: `import_verified` and `materialize` acquire their own digest
//! lock. Do not call them while holding `lock_object` for that digest: locks are
//! not reentrant across independent file handles. The explicit lock API is for
//! low-level coordination and interoperability checks, not a wrapper around
//! those already-locked methods.
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const DEFAULT_DIRECTORY: &str = ".jmclcore";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    HardLink,
    Copy,
}
#[derive(Debug, Clone, Copy)]
pub struct GcOptions {
    pub dry_run: bool,
    pub min_age_seconds: u64,
}
impl Default for GcOptions {
    fn default() -> Self {
        Self {
            dry_run: true,
            min_age_seconds: 86400,
        }
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GcSummary {
    pub objects_scanned: u64,
    pub objects_eligible: u64,
    pub objects_deleted: u64,
    pub bytes_eligible: u64,
    pub bytes_deleted: u64,
    pub objects_linked: u64,
    pub objects_young: u64,
    pub objects_ignored: u64,
    pub objects_busy: u64,
    pub errors: u64,
}
pub fn object_path(hash: &str) -> std::io::Result<PathBuf> {
    let hash = normalized_hash(hash)?;
    Ok(Path::new("objects/sha1").join(&hash[..2]).join(hash))
}
fn normalized_hash(hash: &str) -> std::io::Result<String> {
    if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "InvalidSha1",
        ));
    }
    Ok(hash.to_ascii_lowercase())
}
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::path_from_bytes;
#[cfg(unix)]
pub use unix::{ArtifactRoot, ObjectLock, StagedObject, Store};
#[cfg(windows)]
pub(crate) mod windows;
#[cfg(windows)]
pub(crate) use windows::path_from_bytes;
#[cfg(windows)]
pub use windows::{ArtifactRoot, ObjectLock, StagedObject, Store};

/// One digest transaction, tied to the store which owns its OS lock. Keeps
/// downloads, publication and materialization under the same non-reentrant lock.
pub struct ObjectTransaction<'a> {
    store: &'a Store,
    hash: String,
    guard: ObjectLock,
}
impl Store {
    pub fn transaction(&self, hash: &str) -> std::io::Result<ObjectTransaction<'_>> {
        let hash = normalized_hash(hash)?;
        let guard = self.lock_object(&hash)?;
        Ok(ObjectTransaction {
            store: self,
            hash,
            guard,
        })
    }
}
impl ObjectTransaction<'_> {
    pub fn verify(&self, size: u64) -> std::io::Result<bool> {
        self.store.verify(&self.hash, size)
    }
    pub fn import_from(
        &self,
        source: &ArtifactRoot,
        relative: &str,
        size: u64,
    ) -> std::io::Result<Placement> {
        let (dir, name) = source.parent(relative, false)?;
        self.store
            .import_at_locked(&dir, name.as_ref(), &self.hash, size)
    }
    pub fn materialize_at(
        &self,
        destination: &ArtifactRoot,
        relative: &str,
        size: u64,
    ) -> std::io::Result<Placement> {
        let (dir, name) = destination.parent(relative, true)?;
        self.store
            .materialize_at_locked(&dir, name.as_ref(), &self.hash, size)
    }
    pub(crate) fn import_bytes(
        &self,
        source: &ArtifactRoot,
        relative: &[u8],
        size: u64,
    ) -> std::io::Result<Placement> {
        let (dir, name) = source.content_parent_bytes(relative, false)?;
        self.store.import_at_locked(&dir, &name, &self.hash, size)
    }
    pub(crate) fn materialize_bytes(
        &self,
        destination: &ArtifactRoot,
        relative: &[u8],
        size: u64,
    ) -> std::io::Result<Placement> {
        let (dir, name) = destination.content_parent_bytes(relative, true)?;
        self.store
            .materialize_at_locked(&dir, &name, &self.hash, size)
    }
    /// Remove a failed verification result while the digest lock is held, as
    /// the legacy installer does before reacquiring the object.
    pub fn remove_invalid(&self, size: u64) -> std::io::Result<()> {
        if !self.verify(size)? {
            self.store.remove_object_locked(&self.hash)?;
        }
        Ok(())
    }
    pub fn stage(&self, size: u64) -> std::io::Result<StagedObject<'_>> {
        self.store.stage_locked(&self.hash, size, &self.guard)
    }
}
fn relative_parts(path: &str) -> std::io::Result<Vec<&str>> {
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "UnsafeRelativePath",
        ));
    }
    Ok(path.split('/').collect())
}

mod commands;
pub use commands::{cli, handle};
fn native_parts(path: &[u8]) -> std::io::Result<Vec<&[u8]>> {
    let parts: Vec<_> = path.split(|b| *b == b'/').collect();
    if path.iter().any(|b| matches!(b, b'\\' | b':' | 0))
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == b"." || *p == b"..")
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "UnsafeRelativePath",
        ));
    }
    Ok(parts)
}

#[cfg(test)]
mod content_byte_bridge_tests {
    use super::*;
    #[test]
    fn content_bytes_missing_source_parent_never_creates_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("content");
        let root = ArtifactRoot::open(&root_path).unwrap();
        let store = Store::open(temp.path().join("cache")).unwrap();
        let tx = store.transaction(&"a".repeat(40)).unwrap();
        let error = tx
            .import_bytes(&root, b"missing/child/file.jar", 1)
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(!root_path.join("missing").exists());
        for path in [
            b"../outside".as_slice(),
            b"/outside",
            b"x//file",
            b"x\\file",
            b"x:\0",
        ] {
            assert_eq!(
                tx.import_bytes(&root, path, 1).unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput
            );
        }
        assert_eq!(std::fs::read_dir(&root_path).unwrap().count(), 0);
    }
    #[cfg(unix)]
    #[test]
    fn content_bytes_source_parent_alias_is_not_followed_by_cas() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("content");
        let root = ArtifactRoot::open(&root_path).unwrap();
        let outside = temp.path().join("peer");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("opaque"), b"unchanged").unwrap();
        std::os::unix::fs::symlink(&outside, root_path.join("alias")).unwrap();
        let store = Store::open(temp.path().join("cache")).unwrap();
        let tx = store.transaction(&"a".repeat(40)).unwrap();
        assert!(tx.import_bytes(&root, b"alias/opaque", 9).is_err());
        assert_eq!(std::fs::read(outside.join("opaque")).unwrap(), b"unchanged");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    }
}

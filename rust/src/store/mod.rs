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

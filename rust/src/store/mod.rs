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
pub use unix::{ObjectLock, Store};
#[cfg(windows)]
pub(crate) mod windows;
#[cfg(windows)]
pub use windows::{ObjectLock, Store};

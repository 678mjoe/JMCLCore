//! Descriptor-relative world operations. World content is always privately copied,
//! never hard-linked into the immutable artifact store.
use super::{Error, Result};
pub use rustix::fs::FileType;
use rustix::fs::{self as fs, AtFlags, Mode, OFlags};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
pub struct Dir(pub File);
fn cv<T>(x: rustix::io::Result<T>) -> Result<T> {
    x.map_err(|e| Error::Io(e.into()))
}
impl Dir {
    pub fn root(path: &Path) -> Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(Error::InvalidName);
        }
        std::fs::create_dir_all(path)?;
        Ok(Self(File::from(cv(fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        ))?)))
    }
    pub fn child(&self, name: impl AsRef<OsStr>, create: bool) -> Result<Self> {
        let n = name.as_ref();
        if create {
            match fs::mkdirat(&self.0, n, Mode::from_raw_mode(0o755)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(e) => return Err(Error::Io(e.into())),
            }
        }
        Ok(Self(File::from(cv(fs::openat(
            &self.0,
            n,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ))?)))
    }
    pub fn child_alias(&self, name: &str) -> Result<Self> {
        Ok(Self(File::from(cv(fs::openat(
            &self.0,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        ))?)))
    }
    pub fn new_child(&self, name: &str) -> Result<Self> {
        cv(fs::mkdirat(&self.0, name, Mode::from_raw_mode(0o755)))?;
        self.child(name, false)
    }
    pub fn file(&self, name: impl AsRef<OsStr>, write: bool) -> Result<File> {
        let flags = if write { OFlags::RDWR } else { OFlags::RDONLY };
        let f = File::from(cv(fs::openat(
            &self.0,
            name.as_ref(),
            flags | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ))?);
        if !f.metadata()?.is_file() {
            return Err(Error::UnsupportedEntry);
        }
        Ok(f)
    }
    pub fn names(&self) -> Result<Vec<(OsString, FileType)>> {
        let mut it = cv(fs::Dir::read_from(&self.0))?;
        let mut v = vec![];
        while let Some(e) = it.read() {
            let e = cv(e)?;
            let n = e.file_name().to_bytes();
            if n != b"." && n != b".." {
                v.push((OsString::from_vec(n.to_vec()), e.file_type()))
            }
        }
        Ok(v)
    }
    pub fn exists(&self, name: &str) -> bool {
        fs::statat(&self.0, name, AtFlags::empty()).is_ok()
    }
    // Named compatibility aliases are opened without mutating their bytes.
    pub fn file_alias(&self, name: &str, write: bool) -> Result<File> {
        let f = File::from(cv(fs::openat(
            &self.0,
            name,
            (if write { OFlags::RDWR } else { OFlags::RDONLY })
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::empty(),
        ))?);
        if !f.metadata()?.is_file() {
            return Err(Error::UnsupportedEntry);
        }
        Ok(f)
    }
    pub fn read_alias(&self, name: &str, max: usize) -> Result<Vec<u8>> {
        let f = self.file_alias(name, false)?;
        let mut bytes = vec![];
        f.take(max as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(Error::ArchiveTooLarge);
        }
        Ok(bytes)
    }
    pub fn read(&self, name: &str, max: usize) -> Result<Vec<u8>> {
        let f = self.file(name, false)?;
        let mut bytes = vec![];
        f.take(max as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(Error::ArchiveTooLarge);
        }
        Ok(bytes)
    }
    pub fn new_file(&self, name: &str) -> Result<File> {
        Ok(File::from(cv(fs::openat(
            &self.0,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        ))?))
    }
    pub fn write_new(&self, name: &str, data: &[u8]) -> Result<()> {
        let mut f = File::from(cv(fs::openat(
            &self.0,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        ))?);
        f.write_all(data)?;
        f.sync_all()?;
        Ok(())
    }
    pub fn write_atomic(&self, name: &str, data: &[u8]) -> Result<()> {
        let tmp = super::unique(".jmcl-write-", name)?;
        self.write_new(&tmp, data)?;
        let r = self.rename(&tmp, self, name);
        if r.is_err() {
            let _ = self.unlink(&tmp);
        }
        r
    }
    pub fn rename(&self, old: &str, to: &Dir, new: &str) -> Result<()> {
        cv(fs::renameat(&self.0, old, &to.0, new))
    }
    pub fn unlink(&self, name: impl AsRef<OsStr>) -> Result<()> {
        cv(fs::unlinkat(&self.0, name.as_ref(), AtFlags::empty()))
    }
    pub fn remove_tree(&self, name: impl AsRef<OsStr>) -> Result<()> {
        let n = name.as_ref();
        let stat = cv(fs::statat(&self.0, n, AtFlags::SYMLINK_NOFOLLOW))?;
        if FileType::from_raw_mode(stat.st_mode) == FileType::Symlink {
            return self.unlink(n);
        }
        let child = self.child(n, false)?;
        for (e, kind) in child.names()? {
            if kind == FileType::Directory {
                child.remove_tree(&e)?
            } else {
                child.unlink(&e)?
            }
        }
        drop(child);
        cv(fs::unlinkat(&self.0, n, AtFlags::REMOVEDIR))
    }
    pub fn size(&self) -> u64 {
        let mut total = 0;
        for (name, t) in self.names().unwrap_or_default() {
            match t {
                FileType::RegularFile => {
                    if let Ok(f) = self.file(&name, false)
                        && let Ok(m) = f.metadata()
                    {
                        total += m.len()
                    }
                }
                FileType::Directory => {
                    if let Ok(d) = self.child(&name, false) {
                        total += d.size()
                    }
                }
                _ => {}
            }
        }
        total
    }
    pub fn locked(&self) -> Result<bool> {
        let f = match self.file_alias("session.lock", true) {
            Ok(f) => f,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match f.try_lock() {
            Ok(()) => {
                f.unlock()?;
                Ok(false)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
        }
    }
    pub fn copy_to(&self, to: &Dir) -> Result<()> {
        for (name, t) in self.names()? {
            let n = name.to_str().ok_or(Error::UnsupportedEntry)?;
            match t {
                FileType::RegularFile => {
                    if n == "session.lock" {
                        continue;
                    }
                    let mut from = self.file(&name, false)?;
                    let mut dest = File::from(cv(fs::openat(
                        &to.0,
                        &name,
                        OFlags::WRONLY
                            | OFlags::CREATE
                            | OFlags::EXCL
                            | OFlags::NOFOLLOW
                            | OFlags::CLOEXEC,
                        Mode::from_raw_mode(0o666),
                    ))?);
                    std::io::copy(&mut from, &mut dest)?;
                    dest.sync_all()?;
                }
                FileType::Directory => {
                    if super::is_temp(n) {
                        continue;
                    }
                    self.child(&name, false)?.copy_to(&to.child(&name, true)?)?
                }
                _ => return Err(Error::UnsupportedEntry),
            }
        }
        Ok(())
    }
    pub fn write_relative(&self, path: &str, data: &[u8]) -> Result<()> {
        let mut components = path
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .collect::<Vec<_>>();
        let name = components.pop().ok_or(Error::ArchiveInvalid)?;
        let mut dir = self.child(".", false)?;
        for part in components {
            dir = dir.child(part, true)?;
        } // Existing duplicate archive entries replace a private staging file, never shared state.
        let mut f = File::from(cv(fs::openat(
            &dir.0,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        ))?);
        f.write_all(data)?;
        Ok(())
    }
    pub fn mkdir_relative(&self, path: &str) -> Result<()> {
        let mut dir = self.child(".", false)?;
        for p in path.split('/').filter(|s| !s.is_empty() && *s != ".") {
            dir = dir.child(p, true)?;
        }
        Ok(())
    }
}

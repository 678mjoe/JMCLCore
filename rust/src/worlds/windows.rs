//! Windows mutable worlds: shared handle-relative primitives, never CAS links.
use super::{Error, Result};
use crate::store::windows as nt;
use std::ffi::{OsStr, OsString};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Directory,
    RegularFile,
    Other,
}
pub struct Dir(pub File);
impl Dir {
    pub fn root(path: &Path) -> Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(Error::InvalidName);
        }
        Ok(Self(nt::walk(path, true)?))
    }
    pub fn child(&self, name: impl AsRef<OsStr>, create: bool) -> Result<Self> {
        if name.as_ref() == OsStr::new(".") {
            return Ok(Self(self.0.try_clone()?));
        }
        Ok(Self(nt::child(
            &self.0,
            name.as_ref(),
            true,
            create,
            false,
        )?))
    }
    pub fn new_child(&self, name: &str) -> Result<Self> {
        Ok(Self(nt::create_dir(&self.0, name.as_ref())?))
    }
    pub fn file(&self, name: impl AsRef<OsStr>, write: bool) -> Result<File> {
        Ok(if write {
            nt::child(&self.0, name.as_ref(), false, false, true)?
        } else {
            nt::read_at(&self.0, name.as_ref())?
        })
    }
    pub fn names(&self) -> Result<Vec<(OsString, FileType)>> {
        Ok(nt::entries(&self.0)?
            .into_iter()
            .map(|(n, a)| {
                (
                    n,
                    if a & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        FileType::Other
                    } else if a & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        FileType::Directory
                    } else {
                        FileType::RegularFile
                    },
                )
            })
            .collect())
    }
    pub fn exists(&self, name: &str) -> bool {
        nt::entry_exists(&self.0, name.as_ref())
    }
    pub fn read_alias(&self, name: &str, max: usize) -> Result<Vec<u8>> {
        let f = nt::read_follow_at(&self.0, name.as_ref())?;
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
        Ok(nt::create_file(&self.0, name.as_ref())?)
    }
    pub fn write_new(&self, name: &str, data: &[u8]) -> Result<()> {
        let mut f = self.new_file(name)?;
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
        Ok(nt::rename_at(&self.0, old.as_ref(), &to.0, new.as_ref())?)
    }
    pub fn unlink(&self, name: impl AsRef<OsStr>) -> Result<()> {
        Ok(nt::unlink(&nt::file_at(&self.0, name.as_ref())?)?)
    }
    pub fn remove_tree(&self, name: impl AsRef<OsStr>) -> Result<()> {
        Ok(nt::remove_tree(&self.0, name.as_ref())?)
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
        let f = match self.file("session.lock", true) {
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
                    let mut dest = to.new_file(n)?;
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
        let mut f = nt::child(&dir.0, name.as_ref(), false, true, true)?;
        f.set_len(0)?;
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

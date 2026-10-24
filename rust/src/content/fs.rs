//! Relative operations preserve the configured-root/leaf behavior of the oracle.
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
pub struct Dir(File);
static NEXT: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
mod platform {
    use super::*;
    use rustix::fs::{self as fs, AtFlags, Mode, OFlags};
    use std::os::unix::ffi::OsStringExt;
    impl Dir {
        pub fn root(path: &str) -> io::Result<Self> {
            std::fs::create_dir_all(path)?;
            Ok(Self(File::from(fs::open(
                Path::new(path),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?)))
        }
        pub fn child(&self, name: &str, create: bool) -> io::Result<Self> {
            if create {
                match fs::mkdirat(&self.0, name, Mode::from_raw_mode(0o755)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(Self(File::from(fs::openat(
                &self.0,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )?)))
        }
        pub fn file(&self, name: &str, write: bool) -> io::Result<File> {
            let f = File::from(fs::openat(
                &self.0,
                name,
                (if write { OFlags::RDWR } else { OFlags::RDONLY })
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            if !f.metadata()?.is_file() {
                return Err(io::Error::other("NotFile"));
            }
            Ok(f)
        }
        pub fn names(&self) -> io::Result<Vec<(String, bool)>> {
            let mut it = fs::Dir::read_from(&self.0)?;
            let mut out = vec![];
            while let Some(e) = it.read() {
                let e = e?;
                let name = e.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                let name = std::ffi::OsString::from_vec(name.to_vec());
                if let Some(s) = name.to_str() {
                    out.push((s.into(), e.file_type() == fs::FileType::RegularFile));
                }
            }
            Ok(out)
        }
        pub fn exists(&self, name: &str) -> io::Result<bool> {
            match fs::statat(&self.0, name, AtFlags::empty()) {
                Ok(_) => Ok(true),
                Err(rustix::io::Errno::NOENT) => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
        pub fn unlink(&self, name: &str) -> io::Result<()> {
            Ok(fs::unlinkat(&self.0, name, AtFlags::empty())?)
        }
        pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            Ok(fs::renameat(&self.0, from, &self.0, to)?)
        }
        pub fn atomic_write(&self, name: &str, data: &[u8]) -> io::Result<()> {
            let tmp = format!(
                ".jmcl-content-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let mut file = File::from(fs::openat(
                &self.0,
                tmp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o666),
            )?);
            let result = (|| {
                file.write_all(data)?;
                file.sync_all()?;
                self.rename(&tmp, name)
            })();
            if result.is_err() {
                let _ = self.unlink(&tmp);
            }
            result
        }
    }
}
#[cfg(windows)]
mod platform {
    use super::*;
    use crate::store::windows as fs;
    use std::ffi::OsStr;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    impl Dir {
        pub fn root(path: &str) -> io::Result<Self> {
            Ok(Self(fs::walk(Path::new(path), true)?))
        }
        pub fn child(&self, name: &str, create: bool) -> io::Result<Self> {
            match fs::read_dir_follow_at(&self.0, OsStr::new(name)) {
                Ok(file) => Ok(Self(file)),
                Err(e) if create && e.kind() == io::ErrorKind::NotFound => Ok(Self(fs::child(
                    &self.0,
                    OsStr::new(name),
                    true,
                    true,
                    false,
                )?)),
                Err(e) => Err(e),
            }
        }
        pub fn file(&self, name: &str, write: bool) -> io::Result<File> {
            if write {
                fs::child(&self.0, OsStr::new(name), false, false, true)
            } else {
                fs::read_follow_at(&self.0, OsStr::new(name))
            }
        }
        pub fn names(&self) -> io::Result<Vec<(String, bool)>> {
            Ok(fs::entries(&self.0)?
                .into_iter()
                .filter_map(|(n, a)| {
                    n.to_str().map(|s| {
                        (
                            s.to_string(),
                            a & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0,
                        )
                    })
                })
                .collect())
        }
        pub fn exists(&self, name: &str) -> io::Result<bool> {
            match self.file(name, false) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e),
            }
        }
        pub fn unlink(&self, name: &str) -> io::Result<()> {
            fs::unlink(&fs::file_at(&self.0, OsStr::new(name))?)
        }
        pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            fs::named_op(
                &fs::file_at(&self.0, OsStr::new(from))?,
                &self.0,
                OsStr::new(to),
                true,
            )
        }
        pub fn atomic_write(&self, name: &str, data: &[u8]) -> io::Result<()> {
            let tmp = format!(
                ".jmcl-content-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let mut f = fs::create_file(&self.0, OsStr::new(&tmp))?;
            let result = (|| {
                f.write_all(data)?;
                f.sync_all()?;
                fs::named_op(&f, &self.0, OsStr::new(name), true)
            })();
            if result.is_err() {
                let _ = fs::unlink(&f);
            }
            result
        }
    }
}
impl Dir {
    pub fn parent(&self, path: &str) -> io::Result<(Self, String)> {
        let mut parts = path.rsplitn(2, '/');
        let leaf = parts.next().ok_or_else(|| io::Error::other("EmptyPath"))?;
        let mut dir = Self(self.0.try_clone()?);
        if let Some(parent) = parts.next() {
            for part in parent.split('/') {
                dir = dir.child(part, false)?;
            }
        }
        Ok((dir, leaf.to_string()))
    }
    pub fn read(&self, name: &str, max: usize) -> io::Result<Vec<u8>> {
        let mut bytes = vec![];
        self.file(name, false)?
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(io::Error::other("RegistryTooLarge"));
        }
        Ok(bytes)
    }
    pub fn locked(&self) -> io::Result<bool> {
        let f = match self.file("session.lock", true) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match f.try_lock() {
            Ok(()) => {
                f.unlock()?;
                Ok(false)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
}

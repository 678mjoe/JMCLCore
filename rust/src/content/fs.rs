//! Relative operations preserve the configured-root/leaf behavior of the oracle.
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
pub struct Dir(File);
#[cfg(target_os = "linux")]
impl Dir {
    pub(crate) fn from_directory(dir: File) -> Self {
        Self(dir)
    }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
mod platform {
    use super::*;
    use rustix::fs::{self as fs, AtFlags, Mode, OFlags};

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
            self.child_bytes(name.as_bytes(), create)
        }
        pub(super) fn child_bytes(&self, bytes: &[u8], create: bool) -> io::Result<Self> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
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
            self.file_bytes(name.as_bytes(), write)
        }
        pub fn file_bytes(&self, bytes: &[u8], write: bool) -> io::Result<File> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
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
        pub fn names(&self) -> io::Result<Vec<(Vec<u8>, bool)>> {
            let mut it = fs::Dir::read_from(&self.0)?;
            let mut out = vec![];
            while let Some(e) = it.read() {
                let e = e?;
                let name = e.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                out.push((name.to_vec(), e.file_type() == fs::FileType::RegularFile));
            }
            Ok(out)
        }
        pub fn exists_bytes(&self, bytes: &[u8]) -> io::Result<bool> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
            match fs::statat(&self.0, name, AtFlags::empty()) {
                Ok(_) => Ok(true),
                Err(rustix::io::Errno::NOENT) => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
        pub fn unlink(&self, name: &str) -> io::Result<()> {
            self.unlink_bytes(name.as_bytes())
        }
        pub fn unlink_bytes(&self, bytes: &[u8]) -> io::Result<()> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
            Ok(fs::unlinkat(&self.0, name, AtFlags::empty())?)
        }
        pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            self.rename_bytes(from.as_bytes(), to.as_bytes())
        }
        pub fn rename_bytes(&self, from: &[u8], to: &[u8]) -> io::Result<()> {
            let from = crate::store::path_from_bytes(from)?;
            let to = crate::store::path_from_bytes(to)?;
            let (from, to) = (from.as_os_str(), to.as_os_str());
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
    pub(super) fn name_bytes(name: &OsStr) -> Vec<u8> {
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
    #[test]
    fn filename_wtf8_is_exact_and_roundtrips() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        for units in [
            vec![0x61],
            vec![0xd800, 0x2e, 0x6a],
            vec![0xdc00],
            vec![0xd83d, 0xde00],
            vec![0xd800, 0xd800],
        ] {
            let name = std::ffi::OsString::from_wide(&units);
            let bytes = name_bytes(&name);
            let decoded = crate::store::path_from_bytes(&bytes).unwrap();
            assert_eq!(decoded.encode_wide().collect::<Vec<_>>(), units);
        }
        assert_eq!(
            name_bytes(&std::ffi::OsString::from_wide(&[0xd800])),
            b"\xed\xa0\x80"
        );
    }
    impl Dir {
        pub fn root(path: &str) -> io::Result<Self> {
            Ok(Self(fs::walk(Path::new(path), true)?))
        }
        pub fn child(&self, name: &str, create: bool) -> io::Result<Self> {
            self.child_bytes(name.as_bytes(), create)
        }
        pub(super) fn child_bytes(&self, bytes: &[u8], create: bool) -> io::Result<Self> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
            match fs::read_dir_follow_at(&self.0, name) {
                Ok(file) => Ok(Self(file)),
                Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                    Ok(Self(fs::child(&self.0, name, true, true, false)?))
                }
                Err(e) => Err(e),
            }
        }
        pub fn file(&self, name: &str, write: bool) -> io::Result<File> {
            self.file_bytes(name.as_bytes(), write)
        }
        pub fn file_bytes(&self, bytes: &[u8], write: bool) -> io::Result<File> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
            if write {
                fs::child(&self.0, name, false, false, true)
            } else {
                fs::read_follow_at(&self.0, name)
            }
        }
        pub fn names(&self) -> io::Result<Vec<(Vec<u8>, bool)>> {
            Ok(fs::entries(&self.0)?
                .into_iter()
                .map(|(n, a)| {
                    (
                        name_bytes(&n),
                        a & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0,
                    )
                })
                .collect())
        }
        pub fn exists_bytes(&self, bytes: &[u8]) -> io::Result<bool> {
            match self.file_bytes(bytes, false) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e),
            }
        }
        pub fn unlink_bytes(&self, bytes: &[u8]) -> io::Result<()> {
            let name = crate::store::path_from_bytes(bytes)?;
            let name = name.as_os_str();
            fs::unlink(&fs::file_at(&self.0, name)?)
        }
        pub fn rename_bytes(&self, from: &[u8], to: &[u8]) -> io::Result<()> {
            let from = crate::store::path_from_bytes(from)?;
            let to = crate::store::path_from_bytes(to)?;
            let (from, to) = (from.as_os_str(), to.as_os_str());
            fs::named_op(&fs::file_at(&self.0, from)?, &self.0, to, true)
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
    pub fn artifact_root(&self) -> io::Result<crate::store::ArtifactRoot> {
        crate::store::ArtifactRoot::from_directory(self.0.try_clone()?)
    }
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
    pub fn parent_bytes(&self, path: &[u8]) -> io::Result<(Self, Vec<u8>)> {
        let mut parts = path.rsplitn(2, |b| *b == b'/');
        let leaf = parts.next().ok_or_else(|| io::Error::other("EmptyPath"))?;
        let mut dir = Self(self.0.try_clone()?);
        if let Some(parent) = parts.next() {
            for part in parent.split(|b| *b == b'/') {
                dir = dir.child_bytes(part, false)?;
            }
        }
        Ok((dir, leaf.to_vec()))
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

/// Exact OS argument bytes: Unix bytes or Windows WTF-8, never lossy text.
pub(super) fn argument_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        name.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        platform::name_bytes(name)
    }
}

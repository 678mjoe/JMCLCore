use super::{MAX_MANIFEST_BYTES, Manifest, installed_bytes, validate_id};
use rustix::fs::{self as fs, AtFlags, FileType, Mode, OFlags};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Write},
    os::unix::ffi::OsStrExt,
    sync::atomic::{AtomicU64, Ordering},
};
pub struct Root {
    dir: File,
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s.to_owned())
}
fn cv<T>(r: rustix::io::Result<T>) -> io::Result<T> {
    r.map_err(Into::into)
}
fn open_dir(parent: &File, name: &OsStr, create: bool) -> io::Result<File> {
    if create {
        match fs::mkdirat(parent, name, Mode::from_raw_mode(0o755)) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(File::from(cv(fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ))?))
}
fn read(dir: &File, name: &str, max: usize) -> io::Result<Vec<u8>> {
    read_policy(dir, name, max, false)
}
fn read_policy(dir: &File, name: &str, max: usize, follow_leaf: bool) -> io::Result<Vec<u8>> {
    let f = File::from(cv(fs::openat(
        dir,
        name,
        OFlags::RDONLY
            | OFlags::CLOEXEC
            | OFlags::NONBLOCK
            | if follow_leaf {
                OFlags::empty()
            } else {
                OFlags::NOFOLLOW
            },
        Mode::empty(),
    ))?);
    if !f.metadata()?.is_file() {
        return Err(invalid("NotFile"));
    }
    let mut b = Vec::new();
    f.take(max as u64 + 1).read_to_end(&mut b)?;
    if b.len() > max {
        return Err(invalid("TooLarge"));
    }
    Ok(b)
}
fn atomic_write(dir: &File, name: &str, data: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp = format!(
        ".jmcl-manifest-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let mut f = File::from(cv(fs::openat(
        dir,
        temp.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o666),
    ))?);
    let result = (|| {
        f.write_all(data)?;
        f.sync_all()?;
        cv(fs::renameat(dir, temp.as_str(), dir, name))
    })();
    if result.is_err() {
        let _ = fs::unlinkat(dir, temp.as_str(), AtFlags::empty());
    }
    result
}
fn names(dir: &File) -> io::Result<Vec<(OsString, FileType)>> {
    let entries = cv(fs::Dir::read_from(dir))?;
    let mut result = Vec::new();
    for e in entries {
        let e = cv(e)?;
        if matches!(e.file_name().to_bytes(), b"." | b"..") {
            continue;
        }
        result.push((
            OsStr::from_bytes(e.file_name().to_bytes()).to_owned(),
            e.file_type(),
        ));
    }
    Ok(result)
}
fn remove_tree(parent: &File, name: &OsStr) -> io::Result<()> {
    let dir = match open_dir(parent, name, false) {
        Ok(d) => d,
        Err(e) if matches!(e.raw_os_error(), Some(20 | 40)) => {
            return cv(fs::unlinkat(parent, name, AtFlags::empty()));
        }
        Err(e) => return Err(e),
    };
    for (child, _) in names(&dir)? {
        match open_dir(&dir, &child, false) {
            Ok(_) => remove_tree(&dir, &child)?,
            Err(_) => cv(fs::unlinkat(&dir, &child, AtFlags::empty()))?,
        }
    }
    cv(fs::unlinkat(parent, name, AtFlags::REMOVEDIR))
}
impl Root {
    pub fn open(path: &str) -> io::Result<Self> {
        if path.is_empty() {
            return Err(invalid("EmptyInstancesRoot"));
        }
        // Configured roots are caller-selected. Match Zig: allow symlink ancestors
        // (including macOS /tmp and /var), but refuse a symlink final directory.
        std::fs::create_dir_all(path)?;
        let dir = File::from(cv(fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ))?);
        Ok(Self { dir })
    }
    pub fn create(&self, m: &Manifest) -> io::Result<()> {
        m.validate().map_err(invalid)?;
        cv(fs::mkdirat(
            &self.dir,
            m.id.as_str(),
            Mode::from_raw_mode(0o755),
        ))?;
        let result = (|| {
            let child = open_dir(&self.dir, m.id.as_ref(), false)?;
            cv(fs::mkdirat(
                &child,
                ".minecraft",
                Mode::from_raw_mode(0o755),
            ))?;
            atomic_write(&child, "instance.json", &m.encode().map_err(invalid)?)
        })();
        if result.is_err() {
            let _ = remove_tree(&self.dir, m.id.as_ref());
        }
        result
    }
    pub fn load(&self, id: &str) -> io::Result<Manifest> {
        validate_id(id).map_err(invalid)?;
        let child = open_dir(&self.dir, id.as_ref(), false)?;
        Manifest::parse(
            &read_policy(&child, "instance.json", MAX_MANIFEST_BYTES, true).map_err(|e| {
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
    pub fn list(&self) -> io::Result<Vec<Manifest>> {
        let mut list = Vec::new();
        for (name, kind) in names(&self.dir)? {
            if !matches!(kind, FileType::Directory | FileType::Unknown) {
                continue;
            }
            let Some(id) = name.to_str() else { continue };
            if validate_id(id).is_err() {
                continue;
            }
            if list.len() == 4096 {
                return Err(invalid("TooManyInstances"));
            }
            if let Ok(m) = self.load(id) {
                list.push(m)
            }
        }
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }
    pub fn delete(&self, id: &str) -> io::Result<()> {
        self.load(id)?;
        remove_tree(&self.dir, id.as_ref())
    }
    pub fn installed(&self, m: &Manifest) -> io::Result<bool> {
        let child = open_dir(&self.dir, m.id.as_ref(), false)?;
        let game = match open_dir(&child, ".minecraft".as_ref(), false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        let meta = match open_dir(&game, ".jmcl".as_ref(), false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match read(&meta, "install.json", 16 * 1024) {
            Ok(b) => Ok(installed_bytes(&b, m)),
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
    fn m(id: &str) -> Manifest {
        Manifest {
            schema_version: 1,
            id: id.into(),
            name: "Test".into(),
            version_id: "1.21".into(),
            fabric_loader: None,
            neoforge_version: None,
            forge_version: None,
            source: "official".into(),
        }
    }
    #[test]
    fn lifecycle_preserves_layout() {
        let t = tempfile::tempdir().unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        root.create(&m("z")).unwrap();
        root.create(&m("a")).unwrap();
        assert!(t.path().join("a/.minecraft").is_dir());
        assert_eq!(
            root.list()
                .unwrap()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert!(!root.installed(&m("a")).unwrap());
        root.delete("a").unwrap();
        assert!(!t.path().join("a").exists());
    }
    #[test]
    fn rejects_directory_symlink_without_touching_target() {
        use std::os::unix::fs::symlink;
        let t = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("keep"), b"safe").unwrap();
        symlink(other.path(), t.path().join("a")).unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        assert!(root.load("a").is_err());
        assert!(root.delete("a").is_err());
        assert!(root.list().unwrap().is_empty());
        assert_eq!(std::fs::read(other.path().join("keep")).unwrap(), b"safe");
    }
}

use super::{
    detect::Error,
    receipt::{self, Receipt},
};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read, Write},
    path::Path,
};
#[derive(Debug)]
pub struct Document {
    pub name: String,
    pub receipt: Receipt,
}
fn error(e: io::Error) -> Error {
    match e.kind() {
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        _ => "Unexpected",
    }
}
#[cfg(unix)]
mod fs {
    use super::*;
    use rustix::fs::{self as r, AtFlags, FileType, Mode, OFlags};
    use std::os::unix::ffi::OsStrExt;
    fn cv<T>(v: rustix::io::Result<T>) -> io::Result<T> {
        v.map_err(Into::into)
    }
    pub fn root(path: &Path) -> io::Result<File> {
        std::fs::create_dir_all(path)?;
        Ok(File::from(cv(r::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ))?))
    }
    pub fn dir(parent: &File, name: &OsStr, create: bool) -> io::Result<File> {
        if create {
            match r::mkdirat(parent, name, Mode::from_raw_mode(0o755)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(File::from(cv(r::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ))?))
    }
    pub fn read(parent: &File, name: &OsStr) -> io::Result<File> {
        Ok(File::from(cv(r::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ))?))
    }
    pub fn entries(parent: &File) -> io::Result<Vec<(std::ffi::OsString, bool)>> {
        let mut result = vec![];
        for entry in cv(r::Dir::read_from(parent))? {
            let e = cv(entry)?;
            if matches!(e.file_name().to_bytes(), b"." | b"..") {
                continue;
            }
            result.push((
                OsStr::from_bytes(e.file_name().to_bytes()).to_owned(),
                e.file_type() == FileType::Directory,
            ));
        }
        Ok(result)
    }
    pub fn remove(parent: &File, name: &OsStr) -> io::Result<()> {
        let child = match dir(parent, name, false) {
            Ok(d) => d,
            Err(e) if matches!(e.raw_os_error(), Some(20 | 40)) => {
                return cv(r::unlinkat(parent, name, AtFlags::empty()));
            }
            Err(e) => return Err(e),
        };
        for (name, isdir) in entries(&child)? {
            if isdir {
                remove(&child, &name)?
            } else {
                cv(r::unlinkat(&child, &name, AtFlags::empty()))?
            }
        }
        cv(r::unlinkat(parent, name, AtFlags::REMOVEDIR))
    }
    pub fn write_receipt(parent: &File, receipt: &Receipt) -> io::Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let (name, mut temp) = loop {
            let name = format!(
                ".runtime-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            match r::openat(
                parent,
                name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o644),
            ) {
                Ok(fd) => break (name, File::from(fd)),
                Err(rustix::io::Errno::EXIST) => continue,
                Err(e) => return Err(e.into()),
            }
        };
        let result = (|| {
            serde_json::to_writer(&mut temp, receipt)?;
            temp.flush()?;
            temp.sync_all()?;
            cv(r::renameat(parent, name.as_str(), parent, receipt::NAME))
        })();
        if result.is_err() {
            let _ = r::unlinkat(parent, name.as_str(), AtFlags::empty());
        }
        result
    }
}
#[cfg(windows)]
mod fs {
    use super::*;
    use crate::store::windows as w;
    pub fn root(path: &Path) -> io::Result<File> {
        w::walk(path, true)
    }
    pub fn dir(parent: &File, name: &OsStr, create: bool) -> io::Result<File> {
        w::child(parent, name, true, create, false)
    }
    pub fn read(parent: &File, name: &OsStr) -> io::Result<File> {
        w::read_follow_at(parent, name)
    }
    pub fn entries(parent: &File) -> io::Result<Vec<(std::ffi::OsString, bool)>> {
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        };
        Ok(w::entries(parent)?
            .into_iter()
            .map(|(n, a)| {
                (
                    n,
                    a & FILE_ATTRIBUTE_DIRECTORY != 0 && a & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                )
            })
            .collect())
    }
    pub fn remove(parent: &File, name: &OsStr) -> io::Result<()> {
        w::remove_tree(parent, name)
    }
    pub fn write_receipt(parent: &File, receipt: &Receipt) -> io::Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut temp = loop {
            let name = format!(
                ".runtime-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            match w::create_file(parent, name.as_ref()) {
                Ok(f) => break f,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        };
        let result = (|| {
            serde_json::to_writer(&mut temp, receipt)?;
            temp.flush()?;
            temp.sync_all()?;
            w::named_op(&temp, parent, receipt::NAME.as_ref(), true)
        })();
        if result.is_err() {
            let _ = w::unlink(&temp);
        }
        result
    }
}
fn open(store: &str) -> Result<File, Error> {
    if store.is_empty() {
        return Err("EmptyInstallRoot");
    };
    let root = fs::root(Path::new(store)).map_err(error)?;
    fs::dir(&root, OsStr::new("runtimes"), true).map_err(error)
}
pub fn list(store: &str) -> Result<Vec<Document>, Error> {
    let root = open(store)?;
    let entries = fs::entries(&root).map_err(error)?;
    for (name, isdir) in &entries {
        if *isdir && name.to_str().is_some_and(|n| n.contains(".staging-")) {
            let _ = fs::remove(&root, name);
        }
    }
    let mut docs = vec![];
    for (name, isdir) in fs::entries(&root).map_err(error)? {
        if !isdir {
            continue;
        }
        let Some(name) = name.to_str() else { continue };
        if !receipt::valid_name(name) || name.contains(".staging-") {
            continue;
        }
        if docs.len() >= receipt::MAX_RUNTIMES {
            return Err("TooManyRuntimes");
        };
        let Ok(dir) = fs::dir(&root, name.as_ref(), false) else {
            continue;
        };
        let Ok(file) = fs::read(&dir, receipt::NAME.as_ref()) else {
            continue;
        };
        let mut bytes = vec![];
        if file
            .take((receipt::MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        let Ok(receipt) = receipt::parse(&bytes) else {
            continue;
        };
        docs.push(Document {
            name: name.into(),
            receipt,
        })
    }
    docs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(docs)
}
// A separate listing view deliberately leaves managed selection on list/Receipt.
#[derive(Debug)]
pub(super) struct ReadDocument {
    pub name: String,
    pub receipt: receipt::ReadReceipt,
}
pub(super) fn list_read(store: &str) -> Result<Vec<ReadDocument>, Error> {
    let root = open(store)?;
    let entries = fs::entries(&root).map_err(error)?;
    for (name, isdir) in &entries {
        if *isdir && name.to_str().is_some_and(|n| n.contains(".staging-")) {
            let _ = fs::remove(&root, name);
        }
    }
    let mut docs = vec![];
    for (name, isdir) in fs::entries(&root).map_err(error)? {
        if !isdir {
            continue;
        }
        let Some(name) = name.to_str() else { continue };
        if !receipt::valid_name(name) || name.contains(".staging-") {
            continue;
        }
        if docs.len() >= receipt::MAX_RUNTIMES {
            return Err("TooManyRuntimes");
        };
        let Ok(dir) = fs::dir(&root, name.as_ref(), false) else {
            continue;
        };
        let Ok(file) = fs::read(&dir, receipt::NAME.as_ref()) else {
            continue;
        };
        let mut bytes = vec![];
        if file
            .take((receipt::MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        let Ok(receipt) = receipt::parse_read(&bytes) else {
            continue;
        };
        docs.push(ReadDocument {
            name: name.into(),
            receipt,
        })
    }
    docs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(docs)
}
pub fn remove(store: &str, name: &str) -> Result<(), Error> {
    let root = open(store)?;
    if !receipt::valid_name(name) || name.contains(".staging-") {
        return Err("InvalidRuntimeName");
    };
    let handle = fs::dir(&root, name.as_ref(), false).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            "RuntimeNotFound"
        } else {
            error(e)
        }
    })?;
    drop(handle);
    fs::remove(&root, name.as_ref()).map_err(error)
}
/// Write a validated receipt only into an already-created installer staging directory.
/// Installation publication is a separate transaction; this does not install Java.
pub fn write_receipt(runtime_directory: &Path, receipt: &Receipt) -> Result<(), Error> {
    receipt.validate()?;
    let root = fs::root(runtime_directory).map_err(error)?;
    fs::write_receipt(&root, receipt).map_err(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_ignores_invalid_and_cleans_staging() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("store");
        let data = r#"{"provider":"mojang","id":"java-runtime-gamma","platform":"linux","major_version":17,"java_home":"","executable":"bin/java","source_url":"https://example.invalid/runtime","installed_at_ms":123}"#;
        for (name, bytes) in [
            ("mojang-a-linux", data),
            ("broken", "oops"),
            ("x.staging-123", data),
        ] {
            let p = root.join("runtimes").join(name);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join(receipt::NAME), bytes).unwrap()
        }
        let docs = list(root.to_str().unwrap()).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name, "mojang-a-linux");
        assert!(!root.join("runtimes/x.staging-123").exists());
        remove(root.to_str().unwrap(), "mojang-a-linux").unwrap();
        assert!(list(root.to_str().unwrap()).unwrap().is_empty());
        assert_eq!(
            remove(root.to_str().unwrap(), "missing").unwrap_err(),
            "RuntimeNotFound"
        );
    }
    #[cfg(unix)]
    #[test]
    fn receipt_read_alias_but_delete_preserves_external() {
        use std::os::unix::fs::symlink;
        let t = tempfile::tempdir().unwrap();
        let data = r#"{"provider":"zulu","id":"x","platform":"linux","major_version":8,"java_home":"","executable":"bin/java","source_url":"https://example.invalid/x","installed_at_ms":0}"#;
        let external = t.path().join("outside");
        std::fs::write(&external, data).unwrap();
        let store = t.path().join("store");
        let runtime = store.join("runtimes/zulu-x-linux");
        std::fs::create_dir_all(&runtime).unwrap();
        symlink(&external, runtime.join(receipt::NAME)).unwrap();
        assert_eq!(list(store.to_str().unwrap()).unwrap().len(), 1);
        remove(store.to_str().unwrap(), "zulu-x-linux").unwrap();
        assert_eq!(std::fs::read_to_string(external).unwrap(), data);
    }
}

use super::*;
use rustix::fs::{self as rfs, AtFlags, Mode, OFlags};
use sha1::{Digest, Sha1};
use std::fs::{File, Metadata, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

#[cfg(test)]
type GcBeforeLock = Box<dyn Fn(&str) -> io::Result<()> + Send + Sync>;

pub struct Store {
    root: File,
    #[cfg(test)]
    gc_before_lock: Option<GcBeforeLock>,
}
/// Closing the descriptor releases its OS advisory lock, also used by Zig.
pub struct ObjectLock {
    _file: File,
}
fn cv<T>(r: rustix::io::Result<T>) -> io::Result<T> {
    r.map_err(Into::into)
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
// Unit-test diagnostics preserve the exact OS error and never retry. Production
// builds neither emit filesystem details nor change protocol error names.
fn lock_phase<T>(phase: &str, result: io::Result<T>) -> io::Result<T> {
    #[cfg(test)]
    if let Err(error) = &result {
        eprintln!(
            "CAS phase {phase}, thread {:?}: {error:?}",
            std::thread::current().id()
        );
    }
    #[cfg(not(test))]
    let _ = phase;
    result
}
fn dir_at(parent: &File, name: &std::ffi::OsStr, create: bool) -> io::Result<File> {
    if create {
        match rfs::mkdirat(parent, name, Mode::from_raw_mode(0o755)) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(e) => {
                #[cfg(test)]
                eprintln!("CAS dir_at mkdir {name:?}: {e:?}");
                return Err(e.into());
            }
        }
    }
    Ok(File::from(
        cv(rfs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ))
        .inspect_err(|_error| {
            #[cfg(test)]
            eprintln!("CAS dir_at open {name:?}, create={create}: {_error:?}");
        })?,
    ))
}
/// Resolve caller-configured ancestors, but reject a symlink at the final root.
/// Subdirectories inside the store continue through `dir_at` with NOFOLLOW.
fn walk(path: &Path, create: bool) -> io::Result<File> {
    if path.as_os_str().is_empty() {
        return Err(invalid("EmptyStoreDirectory"));
    }
    if create {
        std::fs::create_dir_all(path)?;
    }
    Ok(File::from(cv(rfs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ))?))
}
fn split_file(path: &Path, create: bool) -> io::Result<(File, &std::ffi::OsStr)> {
    let name = path.file_name().ok_or_else(|| invalid("InvalidFilePath"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((walk(parent, create)?, name))
}
fn file_at(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    let file = File::from(cv(rfs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ))?);
    if !file.metadata()?.is_file() {
        return Err(invalid("InvalidObjectKind"));
    }
    Ok(file)
}
fn digest(file: &mut File) -> io::Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut h = Sha1::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn matches(file: &mut File, hash: &str, size: u64) -> io::Result<bool> {
    Ok(file.metadata()?.len() == size && digest(file)?.eq_ignore_ascii_case(hash))
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}
fn readonly(file: &File) -> io::Result<()> {
    let mode = file.metadata()?.permissions().mode() & !0o222;
    file.set_permissions(Permissions::from_mode(mode))
}
fn fallback(e: rustix::io::Errno) -> bool {
    matches!(
        e,
        rustix::io::Errno::XDEV
            | rustix::io::Errno::OPNOTSUPP
            | rustix::io::Errno::MLINK
            | rustix::io::Errno::ACCESS
            | rustix::io::Errno::PERM
    )
}
fn unique() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // O_EXCL/linkat prevents overwrite even if another process guesses a name.
    format!(
        "{:x}-{:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let root = walk(path.as_ref(), true)?;
        for name in ["objects", "locks"] {
            let dir = dir_at(&root, name.as_ref(), true)?;
            dir_at(&dir, "sha1".as_ref(), true)?;
        }
        Ok(Self {
            root,
            #[cfg(test)]
            gc_before_lock: None,
        })
    }
    pub fn open_existing(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: walk(path.as_ref(), false)?,
            #[cfg(test)]
            gc_before_lock: None,
        })
    }
    fn objects(&self) -> io::Result<File> {
        dir_at(
            &dir_at(&self.root, "objects".as_ref(), false)?,
            "sha1".as_ref(),
            false,
        )
    }
    fn prefix(&self, hash: &str, create: bool) -> io::Result<File> {
        dir_at(&self.objects()?, hash[..2].as_ref(), create)
    }
    pub fn lock_object(&self, hash: &str) -> io::Result<ObjectLock> {
        let hash = normalized_hash(hash)?;
        let dir = lock_phase(
            "locks/sha1 directory",
            dir_at(
                &lock_phase(
                    "locks directory",
                    dir_at(&self.root, "locks".as_ref(), true),
                )?,
                "sha1".as_ref(),
                true,
            ),
        )?;
        let name = format!("{hash}.lock");
        // Match the oracle's exclusive-create / open-existing split. A single
        // O_CREAT open failed with ENOENT during native macOS contention even
        // though the peer-created leaf was immediately observable. Only EXIST
        // selects the existing-file path: all other errors remain errors.
        let flags = OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let created = rfs::openat(
            &dir,
            name.as_str(),
            flags | OFlags::CREATE | OFlags::EXCL,
            Mode::from_raw_mode(0o666),
        );
        let file = File::from(match created {
            Ok(file) => file,
            Err(rustix::io::Errno::EXIST) => lock_phase(
                "open existing lock leaf",
                cv(rfs::openat(&dir, name.as_str(), flags, Mode::empty())),
            )?,
            Err(error) => return lock_phase("exclusive-create lock leaf", Err(error.into())),
        });
        if !lock_phase("lock leaf metadata", file.metadata())?.is_file() {
            return Err(invalid("InvalidLockKind"));
        }
        lock_phase("File.lock/flock", file.lock())?;
        Ok(ObjectLock { _file: file })
    }
    pub fn verify(&self, hash: &str, size: u64) -> io::Result<bool> {
        let hash = normalized_hash(hash)?;
        let result = (|| {
            let prefix = self.prefix(&hash, false)?;
            matches(&mut file_at(&prefix, hash.as_ref())?, &hash, size)
        })();
        match result {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            other => other,
        }
    }
    /// Only verified immutable artifact files may be imported, never mutable worlds.
    /// The source is made read-only because a successful import shares its inode.
    pub fn import_verified(
        &self,
        source: impl AsRef<Path>,
        hash: &str,
        size: u64,
    ) -> io::Result<Placement> {
        let hash = normalized_hash(hash)?;
        let _lock = self.lock_object(&hash)?;
        let (source_dir, source_name) = split_file(source.as_ref(), false)?;
        self.import_at_locked(&source_dir, source_name, &hash, size)
    }
    pub(super) fn import_at_locked(
        &self,
        source_dir: &File,
        source_name: &std::ffi::OsStr,
        hash: &str,
        size: u64,
    ) -> io::Result<Placement> {
        let mut source = file_at(source_dir, source_name)?;
        if !matches(&mut source, hash, size)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ArtifactVerificationFailed",
            ));
        }
        let prefix = self.prefix(hash, true)?;
        if self.verify(hash, size)? {
            return Ok(Placement::HardLink);
        }
        // Repair publishes a distinct inode: never overwrite a shared object in place.
        let temp = format!(".jmcl-object-{}", unique());
        let placement = match rfs::linkat(
            source_dir,
            source_name,
            &prefix,
            temp.as_str(),
            AtFlags::empty(),
        ) {
            Ok(()) => Placement::HardLink,
            Err(e) if fallback(e) => {
                let mut output = File::from(cv(rfs::openat(
                    &prefix,
                    temp.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o644),
                ))?);
                source.seek(SeekFrom::Start(0))?;
                if let Err(e) = io::copy(&mut source, &mut output).and_then(|_| output.sync_all()) {
                    let _ = rfs::unlinkat(&prefix, temp.as_str(), AtFlags::empty());
                    return Err(e);
                }
                Placement::Copy
            }
            Err(e) => return Err(e.into()),
        };
        let publish = (|| {
            let mut linked = file_at(&prefix, temp.as_ref())?;
            if !matches(&mut linked, hash, size)? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ArtifactVerificationFailed",
                ));
            }
            readonly(&linked)?;
            cv(rfs::renameat(&prefix, temp.as_str(), &prefix, hash))
        })();
        if publish.is_err() {
            let _ = rfs::unlinkat(&prefix, temp.as_str(), AtFlags::empty());
        }
        publish?;
        Ok(placement)
    }
    pub fn materialize(
        &self,
        hash: &str,
        size: u64,
        destination: impl AsRef<Path>,
    ) -> io::Result<Placement> {
        let hash = normalized_hash(hash)?;
        let _lock = self.lock_object(&hash)?;
        let (dest_dir, dest_name) = split_file(destination.as_ref(), true)?;
        self.materialize_at_locked(&dest_dir, dest_name, &hash, size)
    }
    pub(super) fn materialize_at_locked(
        &self,
        dest_dir: &File,
        dest_name: &std::ffi::OsStr,
        hash: &str,
        size: u64,
    ) -> io::Result<Placement> {
        let prefix = self.prefix(hash, false)?;
        let mut source = file_at(&prefix, hash.as_ref())?;
        if !matches(&mut source, hash, size)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ArtifactVerificationFailed",
            ));
        }
        readonly(&source)?;
        let temp = format!("{}.jmcl-link-{}", dest_name.to_string_lossy(), unique());
        match rfs::linkat(&prefix, hash, dest_dir, temp.as_str(), AtFlags::empty()) {
            Ok(()) => {
                let finish = (|| {
                    if let Ok(existing) = file_at(dest_dir, dest_name)
                        && same(&existing.metadata()?, &source.metadata()?)
                    {
                        return Ok(());
                    }
                    cv(rfs::renameat(dest_dir, temp.as_str(), dest_dir, dest_name))
                })();
                let _ = rfs::unlinkat(dest_dir, temp.as_str(), AtFlags::empty());
                finish?;
                Ok(Placement::HardLink)
            }
            Err(e) if fallback(e) => {
                // Match Zig's documented non-atomic copy fallback, without following
                // final-component symlinks or truncating a shared inode.
                let mut output = match rfs::openat(
                    dest_dir,
                    dest_name,
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o644),
                ) {
                    Ok(fd) => File::from(fd),
                    Err(rustix::io::Errno::EXIST) => {
                        let existing = file_at(dest_dir, dest_name)?;
                        if matches(&mut existing.try_clone()?, hash, size)? {
                            return Ok(Placement::Copy);
                        }
                        cv(rfs::unlinkat(dest_dir, dest_name, AtFlags::empty()))?;
                        File::from(cv(rfs::openat(
                            dest_dir,
                            dest_name,
                            OFlags::WRONLY
                                | OFlags::CREATE
                                | OFlags::EXCL
                                | OFlags::NOFOLLOW
                                | OFlags::CLOEXEC,
                            Mode::from_raw_mode(0o644),
                        ))?)
                    }
                    Err(e) => return Err(e.into()),
                };
                source.seek(SeekFrom::Start(0))?;
                io::copy(&mut source, &mut output)?;
                output.flush()?;
                readonly(&output)?;
                Ok(Placement::Copy)
            }
            Err(e) => Err(e.into()),
        }
    }
    pub fn collect(&self, options: GcOptions) -> io::Result<GcSummary> {
        let objects = self.objects()?;
        let mut summary = GcSummary::default();
        for prefix_name in names(&objects)? {
            if prefix_name.len() != 2 || !lower_hex(&prefix_name) {
                summary.objects_ignored += 1;
                continue;
            }
            let prefix = match dir_at(&objects, std::ffi::OsStr::from_bytes(&prefix_name), false) {
                Ok(f) => f,
                Err(_) => {
                    summary.objects_ignored += 1;
                    continue;
                }
            };
            for hash in names(&prefix)? {
                if hash.len() != 40 || !hash.starts_with(&prefix_name) || !lower_hex(&hash) {
                    summary.objects_ignored += 1;
                    continue;
                }
                let name = std::ffi::OsStr::from_bytes(&hash);
                let stat = cv(rfs::statat(&prefix, name, AtFlags::SYMLINK_NOFOLLOW))?;
                if rfs::FileType::from_raw_mode(stat.st_mode) != rfs::FileType::RegularFile {
                    summary.objects_ignored += 1;
                    continue;
                }
                summary.objects_scanned += 1;
                if let Err(error) = self.collect_one(&prefix, name, options, &mut summary) {
                    if error.kind() == io::ErrorKind::NotFound
                        || error.kind() == io::ErrorKind::WouldBlock
                    {
                        summary.objects_busy += 1;
                    } else {
                        summary.errors += 1;
                    }
                }
            }
        }
        Ok(summary)
    }
    fn collect_one(
        &self,
        prefix: &File,
        name: &std::ffi::OsStr,
        options: GcOptions,
        s: &mut GcSummary,
    ) -> io::Result<()> {
        let file = file_at(prefix, name)?;
        let before = file.metadata()?;
        if !eligible(&before, options, s)? {
            return Ok(());
        }
        if options.dry_run {
            s.objects_eligible += 1;
            s.bytes_eligible += before.len();
            return Ok(());
        }
        #[cfg(test)]
        if let Some(hook) = &self.gc_before_lock {
            hook(name.to_str().ok_or_else(|| invalid("InvalidSha1"))?)?;
        }
        let _lock = self.lock_object(name.to_str().ok_or_else(|| invalid("InvalidSha1"))?)?;
        let current = file_at(prefix, name)?;
        let stat = current.metadata()?;
        if !eligible(&stat, options, s)? {
            return Ok(());
        }
        s.objects_eligible += 1;
        s.bytes_eligible += stat.len();
        cv(rfs::unlinkat(prefix, name, AtFlags::empty()))?;
        s.objects_deleted += 1;
        s.bytes_deleted += stat.len();
        Ok(())
    }
}
fn names(dir: &File) -> io::Result<Vec<Vec<u8>>> {
    let mut names = Vec::new();
    let mut iterator = cv(rfs::Dir::read_from(dir))?;
    while let Some(entry) = iterator.read() {
        let entry = cv(entry)?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            names.push(name.to_vec());
        }
    }
    Ok(names)
}
fn lower_hex(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
fn eligible(stat: &Metadata, options: GcOptions, s: &mut GcSummary) -> io::Result<bool> {
    if stat.nlink() != 1 {
        s.objects_linked += 1;
        return Ok(false);
    }
    let old = SystemTime::now()
        .duration_since(stat.modified()?)
        .map(|d| d >= Duration::from_secs(options.min_age_seconds))
        .unwrap_or(false);
    if !old {
        s.objects_young += 1;
        return Ok(false);
    }
    Ok(true)
}

/// An opened install directory; descendants are always resolved relative to it.
pub struct ArtifactRoot {
    dir: File,
}
impl ArtifactRoot {
    pub(crate) fn from_directory(dir: File) -> io::Result<Self> {
        if !dir.metadata()?.is_dir() {
            return Err(invalid("InvalidObjectKind"));
        }
        Ok(Self { dir })
    }

    pub fn absolute_path(&self) -> io::Result<PathBuf> {
        Self::directory_path(&self.dir)
    }
    pub fn absolute_directory(&self, relative: &str) -> io::Result<PathBuf> {
        let (parent, name) = self.parent(relative, false)?;
        Self::directory_path(&dir_at(&parent, name.as_ref(), false)?)
    }
    fn directory_path(dir: &File) -> io::Result<PathBuf> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            use std::os::fd::AsRawFd;
            std::fs::read_link(format!("/proc/self/fd/{}", dir.as_raw_fd()))
        }
        #[cfg(target_vendor = "apple")]
        {
            Ok(PathBuf::from(std::ffi::OsStr::from_bytes(
                cv(rfs::getpath(dir))?.as_bytes(),
            )))
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
        {
            let _ = dir;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "OperationUnsupported",
            ))
        }
    }
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            dir: walk(path.as_ref(), true)?,
        })
    }
    pub(super) fn parent(&self, relative: &str, create: bool) -> io::Result<(File, String)> {
        let mut parts = relative_parts(relative)?;
        let name = parts.pop().unwrap().to_owned();
        let mut dir = self.dir.try_clone()?;
        for part in parts {
            dir = dir_at(&dir, part.as_ref(), create)?;
        }
        Ok((dir, name))
    }
    pub fn open_file(&self, relative: &str) -> io::Result<File> {
        let (dir, name) = self.parent(relative, false)?;
        file_at(&dir, name.as_ref())
    }
    /// Create an exclusive directory beneath this anchored root.
    pub fn create_directory(&self, relative: &str) -> io::Result<Self> {
        let (parent, name) = self.parent(relative, true)?;
        cv(rfs::mkdirat(
            &parent,
            name.as_str(),
            Mode::from_raw_mode(0o755),
        ))?;
        Ok(Self {
            dir: dir_at(&parent, name.as_ref(), false)?,
        })
    }
    pub fn ensure_directory(&self, relative: &str) -> io::Result<()> {
        let (parent, name) = self.parent(relative, true)?;
        dir_at(&parent, name.as_ref(), true)?;
        Ok(())
    }
    // Content-only bridge: source lookup must never create missing parents.
    pub(super) fn content_parent_bytes(
        &self,
        relative: &[u8],
        create: bool,
    ) -> io::Result<(File, std::ffi::OsString)> {
        let mut parts = native_parts(relative)?;
        let name = path_from_bytes(parts.pop().unwrap())?;
        let mut dir = self.dir.try_clone()?;
        for part in parts {
            dir = dir_at(&dir, &path_from_bytes(part)?, create)?;
        }
        Ok((dir, name))
    }
    fn native_parent(&self, relative: &[u8]) -> io::Result<(File, std::ffi::OsString)> {
        let mut parts = native_parts(relative)?;
        let name = std::ffi::OsStr::from_bytes(parts.pop().unwrap()).to_owned();
        let mut dir = self.dir.try_clone()?;
        for part in parts {
            dir = dir_at(&dir, std::ffi::OsStr::from_bytes(part), true)?;
        }
        Ok((dir, name))
    }
    pub(crate) fn ensure_native_directory(&self, relative: &[u8]) -> io::Result<()> {
        let (parent, name) = self.native_parent(relative)?;
        dir_at(&parent, &name, true)?;
        Ok(())
    }
    pub(crate) fn create_native_file(&self, relative: &[u8]) -> io::Result<File> {
        let (parent, name) = self.native_parent(relative)?;
        Ok(File::from(cv(rfs::openat(
            &parent,
            &name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o644),
        ))?))
    }
    pub fn create_file(&self, relative: &str) -> io::Result<File> {
        let (parent, name) = self.parent(relative, true)?;
        Ok(File::from(cv(rfs::openat(
            &parent,
            name.as_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o644),
        ))?))
    }
    /// Replace mutable bytes with a private inode; never truncate a CAS hardlink.
    pub fn atomic_replace(&self, relative: &str, bytes: &[u8]) -> io::Result<()> {
        let (parent, name) = self.parent(relative, true)?;
        let temp = format!("{}.jmcl-replace-{}", name, unique());
        let result = (|| {
            let mut output = File::from(cv(rfs::openat(
                &parent,
                temp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o644),
            ))?);
            output.write_all(bytes)?;
            output.sync_all()?;
            cv(rfs::renameat(
                &parent,
                temp.as_str(),
                &parent,
                name.as_str(),
            ))
        })();
        let _ = rfs::unlinkat(&parent, temp.as_str(), AtFlags::empty());
        result
    }
    pub fn atomic_replace_bytes(&self, relative: &[u8], bytes: &[u8]) -> io::Result<()> {
        let (parent, name) = self.native_parent(relative)?;
        let temp = format!(".jmcl-replace-{}", unique());
        let result = (|| {
            let mut output = File::from(cv(rfs::openat(
                &parent,
                temp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o644),
            ))?);
            output.write_all(bytes)?;
            output.sync_all()?;
            cv(rfs::renameat(&parent, temp.as_str(), &parent, &name))
        })();
        let _ = rfs::unlinkat(&parent, temp.as_str(), AtFlags::empty());
        result
    }
    pub fn remove_tree(&self, relative: &str) -> io::Result<()> {
        fn remove(parent: &File, name: &std::ffi::OsStr) -> io::Result<()> {
            let stat = cv(rfs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW))?;
            if rfs::FileType::from_raw_mode(stat.st_mode) != rfs::FileType::Directory {
                return cv(rfs::unlinkat(parent, name, AtFlags::empty()));
            }
            let dir = dir_at(parent, name, false)?;
            for item in names(&dir)? {
                remove(&dir, std::ffi::OsStr::from_bytes(&item))?;
            }
            cv(rfs::unlinkat(parent, name, AtFlags::REMOVEDIR))
        }
        let (parent, name) = self.parent(relative, false)?;
        remove(&parent, name.as_ref())
    }
    pub fn remove_file(&self, relative: &str) -> io::Result<()> {
        let (parent, name) = self.parent(relative, false)?;
        cv(rfs::unlinkat(&parent, name.as_str(), AtFlags::empty()))
    }
    pub fn remove_directory(&self, relative: &str) -> io::Result<()> {
        let (parent, name) = self.parent(relative, false)?;
        cv(rfs::unlinkat(&parent, name.as_str(), AtFlags::REMOVEDIR))
    }
    /// Project an immutable verified source, never truncating an existing inode.
    pub fn project(
        &self,
        source_path: &str,
        destination: &str,
        hash: &str,
        size: u64,
    ) -> io::Result<()> {
        let (source_dir, source_name) = self.parent(source_path, false)?;
        let mut source = file_at(&source_dir, source_name.as_ref())?;
        if !matches(&mut source, hash, size)? {
            return Err(invalid("CacheFileInvalid"));
        }
        let (dest, name) = self.parent(destination, true)?;
        let exists = match file_at(&dest, name.as_ref()) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        let temp = format!("{}.jmcl-projection-{}", name, unique());
        let result = (|| {
            if !exists {
                match rfs::linkat(
                    &source_dir,
                    source_name.as_str(),
                    &dest,
                    temp.as_str(),
                    AtFlags::empty(),
                ) {
                    Ok(()) => {
                        let linked = file_at(&dest, temp.as_ref())?;
                        if !same(&linked.metadata()?, &source.metadata()?) {
                            return Err(invalid("CacheFileInvalid"));
                        }
                        return cv(rfs::renameat(&dest, temp.as_str(), &dest, name.as_str()));
                    }
                    Err(rustix::io::Errno::XDEV | rustix::io::Errno::OPNOTSUPP) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            let mut output = File::from(cv(rfs::openat(
                &dest,
                temp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o644),
            ))?);
            source.seek(SeekFrom::Start(0))?;
            if io::copy(&mut source.take(size), &mut output)? != size {
                return Err(invalid("UnexpectedEndOfFile"));
            }
            output.sync_all()?;
            cv(rfs::renameat(&dest, temp.as_str(), &dest, name.as_str()))
        })();
        let _ = rfs::unlinkat(&dest, temp.as_str(), AtFlags::empty());
        result
    }
    /// Content-only byte path verification. Never creates missing parents and
    /// uses the same no-follow regular-file open as the String API.
    pub(crate) fn verify_bytes(&self, relative: &[u8], hash: &str, size: u64) -> io::Result<bool> {
        let opened = self
            .content_parent_bytes(relative, false)
            .and_then(|(dir, name)| file_at(&dir, &name));
        match opened {
            Ok(mut file) => matches(&mut file, hash, size),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
    pub fn verify(&self, relative: &str, hash: &str, size: u64) -> io::Result<bool> {
        match self.open_file(relative) {
            Ok(mut f) => matches(&mut f, hash, size),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}
pub struct StagedObject<'a> {
    file: File,
    dir: File,
    name: String,
    hash: String,
    size: u64,
    published: bool,
    _lock: std::marker::PhantomData<&'a ObjectLock>,
}
impl StagedObject<'_> {
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }
    pub fn publish(mut self, sync: bool) -> io::Result<()> {
        if !matches(&mut self.file, &self.hash, self.size)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ArtifactVerificationFailed",
            ));
        }
        if sync {
            self.file.sync_all()?;
        }
        readonly(&self.file)?;
        cv(rfs::renameat(
            &self.dir,
            self.name.as_str(),
            &self.dir,
            self.hash.as_str(),
        ))?;
        self.published = true;
        Ok(())
    }
}
impl Drop for StagedObject<'_> {
    fn drop(&mut self) {
        if !self.published {
            let _ = rfs::unlinkat(&self.dir, self.name.as_str(), AtFlags::empty());
        }
    }
}
impl Store {
    pub(super) fn remove_object_locked(&self, hash: &str) -> io::Result<()> {
        let result = (|| {
            let dir = self.prefix(hash, false)?;
            cv(rfs::unlinkat(&dir, hash, AtFlags::empty()))
        })();
        match result {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
    pub(super) fn stage_locked<'a>(
        &self,
        hash: &str,
        size: u64,
        _guard: &'a ObjectLock,
    ) -> io::Result<StagedObject<'a>> {
        let dir = self.prefix(hash, true)?;
        let name = format!(".jmcl-download-{}", unique());
        let file = File::from(cv(rfs::openat(
            &dir,
            name.as_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o644),
        ))?);
        Ok(StagedObject {
            file,
            dir,
            name,
            hash: hash.into(),
            size,
            published: false,
            _lock: std::marker::PhantomData,
        })
    }
}

pub(crate) fn path_from_bytes(bytes: &[u8]) -> io::Result<std::ffi::OsString> {
    Ok(std::ffi::OsStr::from_bytes(bytes).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    fn hash(bytes: &[u8]) -> String {
        format!("{:x}", Sha1::digest(bytes))
    }
    fn fixture() -> (tempfile::TempDir, Store, PathBuf, String) {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join(".jmclcore")).unwrap();
        let source = temp.path().join("immutable-source");
        std::fs::write(&source, b"verified artifact").unwrap();
        let h = hash(b"verified artifact");
        (temp, store, source, h)
    }
    #[test]
    #[ignore = "run through compat/cas_gc_recheck.py with an owned-process deadline"]
    fn gc_rechecks_stale_preflight_after_owned_transaction() {
        use std::sync::{Mutex, mpsc};
        fn remove_owned(path: &Path) {
            std::fs::remove_file(path).unwrap();
        }
        fn identity(path: &Path) -> (u64, u64, u64, bool) {
            let m = std::fs::metadata(path).unwrap();
            (m.dev(), m.ino(), m.nlink(), m.permissions().readonly())
        }
        let (temp, store, source, h) = fixture();
        let temp = temp.keep();
        eprintln!("OWNED_GC_FIXTURE: {}", temp.display());
        assert_eq!(
            store.import_verified(&source, &h, 17).unwrap(),
            Placement::HardLink
        );
        remove_owned(&source);
        let cas = temp.as_path().join(".jmclcore");
        let object = cas.join(object_path(&h).unwrap());
        let initial = identity(&object);
        assert_eq!(initial.2, 1);
        assert!(initial.3);
        let root = ArtifactRoot::open(temp.as_path().join("peer")).unwrap();
        let peer = temp.as_path().join("peer/opaque");
        let leaf = cas.join(format!("locks/sha1/{h}.lock"));
        let lock_identity = identity(&leaf);
        let tx = store.transaction(&h).unwrap();
        let descriptor = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&leaf)
            .unwrap();
        assert!(
            matches!(
                descriptor.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ),
            "separate descriptor must report lock contention, not another error"
        );
        let mut collector = Store::open_existing(&cas).unwrap();
        let (arrived, at_preflight) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        let resumed = Mutex::new(resumed);
        let expected_hash = h.clone();
        collector.gc_before_lock = Some(Box::new(move |hash| {
            if hash != expected_hash {
                return Err(io::Error::other("unexpected digest"));
            }
            arrived.send(()).map_err(io::Error::other)?;
            resumed
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .map_err(io::Error::other)
        }));
        let (done, completed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = collector.collect(GcOptions {
                dry_run: false,
                min_age_seconds: 0,
            });
            let _ = done.send(result);
        });
        // Never panic while holding the transaction or withholding the callback.
        let reached = at_preflight.recv_timeout(Duration::from_secs(10));
        let placed = reached
            .as_ref()
            .ok()
            .map(|_| tx.materialize_at(&root, "opaque", 17));
        drop(tx);
        let resumed_ok = resume.send(());
        let collected = completed.recv_timeout(Duration::from_secs(10));
        // The enclosing owned-test-process deadline bounds a pathological join.
        let joined = worker.join();
        assert!(reached.is_ok(), "preflight callback: {reached:?}");
        assert!(resumed_ok.is_ok(), "resume callback: {resumed_ok:?}");
        assert!(joined.is_ok(), "collector thread panicked");
        assert_eq!(placed.unwrap().unwrap(), Placement::HardLink);
        let summary = collected.unwrap().unwrap();
        assert_eq!(
            summary,
            GcSummary {
                objects_scanned: 1,
                objects_linked: 1,
                ..GcSummary::default()
            }
        );
        descriptor
            .try_lock()
            .expect("transaction released descriptor lock");
        descriptor.unlock().unwrap();
        let after = identity(&object);
        assert_eq!(
            (after.0, after.1, after.3),
            (initial.0, initial.1, initial.3)
        );
        assert_eq!(after.2, 2);
        assert_eq!(identity(&peer), after);
        assert_eq!(std::fs::read(&peer).unwrap(), b"verified artifact");
        assert!(store.verify(&h, 17).unwrap());
        assert_eq!(identity(&leaf), lock_identity);
        assert_eq!(std::fs::read(&leaf).unwrap(), b"");
        remove_owned(&peer);
        let removed = store
            .collect(GcOptions {
                dry_run: false,
                min_age_seconds: 0,
            })
            .unwrap();
        assert_eq!(
            removed,
            GcSummary {
                objects_scanned: 1,
                objects_eligible: 1,
                objects_deleted: 1,
                bytes_eligible: 17,
                bytes_deleted: 17,
                ..GcSummary::default()
            }
        );
        assert!(!object.exists());
        assert_eq!(identity(&leaf), lock_identity);
        drop(descriptor);
        drop(root);
        drop(store);
        std::fs::remove_dir_all(&temp).unwrap();
    }
    #[test]
    fn exact_legacy_paths_and_hash_validation() {
        let hash = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        assert_eq!(
            object_path(hash).unwrap(),
            PathBuf::from("objects/sha1/ab/abcdef0123456789abcdef0123456789abcdef01")
        );
        for bad in ["", "../x", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"] {
            assert!(object_path(bad).is_err());
        }
        assert_eq!(GcOptions::default().min_age_seconds, 86400);
        assert!(GcOptions::default().dry_run);
    }
    #[test]
    fn verified_artifacts_share_inode_and_are_read_only() {
        let (temp, store, source, h) = fixture();
        assert_eq!(
            store.import_verified(&source, &h, 17).unwrap(),
            Placement::HardLink
        );
        let object = temp.path().join(".jmclcore").join(object_path(&h).unwrap());
        let one = temp.path().join("instances/one/.minecraft/library.jar");
        let two = temp.path().join("instances/two/.minecraft/library.jar");
        assert_eq!(
            store.materialize(&h, 17, &one).unwrap(),
            Placement::HardLink
        );
        assert_eq!(
            store.materialize(&h, 17, &two).unwrap(),
            Placement::HardLink
        );
        assert_eq!(
            store.materialize(&h, 17, &one).unwrap(),
            Placement::HardLink
        );
        let meta = std::fs::metadata(&object).unwrap();
        assert!(same(&meta, &std::fs::metadata(&source).unwrap()));
        assert!(same(&meta, &std::fs::metadata(&one).unwrap()));
        assert!(same(&meta, &std::fs::metadata(&two).unwrap()));
        assert_eq!(meta.nlink(), 4);
        assert_eq!(meta.permissions().mode() & 0o222, 0);
        std::fs::remove_file(one).unwrap();
        assert_eq!(std::fs::read(two).unwrap(), b"verified artifact");
    }
    #[test]
    fn invalid_bytes_and_sizes_never_publish() {
        let (temp, store, source, h) = fixture();
        assert!(store.import_verified(&source, &h, 16).is_err());
        assert!(store.import_verified(&source, &hash(b"wrong"), 17).is_err());
        assert!(
            !temp
                .path()
                .join(".jmclcore")
                .join(object_path(&h).unwrap())
                .exists()
        );
        assert!(!store.verify(&h, 17).unwrap());
    }
    #[test]
    fn repairs_publish_new_inode_without_changing_other_links() {
        let (temp, store, source, h) = fixture();
        let object = temp.path().join(".jmclcore").join(object_path(&h).unwrap());
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(&object, b"bad").unwrap();
        let old_link = temp.path().join("old-instance");
        std::fs::hard_link(&object, &old_link).unwrap();
        store.import_verified(&source, &h, 17).unwrap();
        assert_eq!(std::fs::read(old_link).unwrap(), b"bad");
        assert_eq!(std::fs::read(object).unwrap(), b"verified artifact");
    }
    #[test]
    fn configured_root_ancestor_alias_is_preserved_but_final_alias_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = temp.path().join("alias");
        symlink(&real, &alias).unwrap();
        let root = alias.join("store");
        let store = Store::open(&root).unwrap();
        let source = alias.join("source");
        std::fs::write(&source, b"shared").unwrap();
        // The explicit source parent itself is an alias, so use its resolved
        // spelling. An ancestor alias in the configured store root is allowed.
        let hash = hash(b"shared");
        store
            .import_verified(real.join("source"), &hash, 6)
            .unwrap();
        assert!(
            real.join("store")
                .join(object_path(&hash).unwrap())
                .exists()
        );
        let final_alias = temp.path().join("final-alias");
        symlink(real.join("store"), &final_alias).unwrap();
        assert!(Store::open_existing(final_alias).is_err());
    }
    #[test]
    fn store_and_destination_symlinks_are_not_followed() {
        let (temp, store, source, h) = fixture();
        store.import_verified(&source, &h, 17).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let linked_dir = temp.path().join("link");
        symlink(&outside, &linked_dir).unwrap();
        assert!(
            store
                .materialize(&h, 17, linked_dir.join("target"))
                .is_err()
        );
        assert!(!outside.join("target").exists());
        assert!(Store::open(&linked_dir).is_err());
        let lock_name = temp.path().join(format!(".jmclcore/locks/sha1/{h}.lock"));
        std::fs::remove_file(&lock_name).unwrap();
        symlink(source, &lock_name).unwrap();
        assert!(store.lock_object(&h).is_err());
    }
    #[test]
    fn gc_preserves_shared_and_mutable_files_then_reclaims_unlinked_object() {
        let (temp, store, source, h) = fixture();
        store.import_verified(&source, &h, 17).unwrap();
        let mutable = temp.path().join("world.dat");
        std::fs::write(&mutable, b"world").unwrap();
        let options = GcOptions {
            dry_run: true,
            min_age_seconds: 0,
        };
        let linked = store.collect(options).unwrap();
        assert_eq!(linked.objects_linked, 1);
        assert_eq!(linked.objects_eligible, 0);
        std::fs::remove_file(source).unwrap();
        let dry = store.collect(options).unwrap();
        assert_eq!(dry.objects_eligible, 1);
        assert_eq!(dry.bytes_eligible, 17);
        assert_eq!(dry.objects_deleted, 0);
        assert!(store.verify(&h, 17).unwrap());
        let deleted = store
            .collect(GcOptions {
                dry_run: false,
                ..options
            })
            .unwrap();
        assert_eq!(deleted.objects_deleted, 1);
        assert_eq!(deleted.bytes_deleted, 17);
        assert!(!store.verify(&h, 17).unwrap());
        assert_eq!(std::fs::read(mutable).unwrap(), b"world");
    }
    #[test]
    fn gc_ignores_malformed_and_symlink_paths() {
        let (temp, store, _, h) = fixture();
        let objects = temp.path().join(".jmclcore/objects/sha1");
        std::fs::create_dir(objects.join(&h[..2])).unwrap();
        std::fs::write(objects.join(&h[..2]).join("junk"), b"x").unwrap();
        symlink("/etc/passwd", objects.join(&h[..2]).join(&h)).unwrap();
        std::fs::write(objects.join("unexpected"), b"x").unwrap();
        let result = store
            .collect(GcOptions {
                dry_run: false,
                min_age_seconds: 0,
            })
            .unwrap();
        assert_eq!(result.objects_ignored, 3);
        assert_eq!(result.objects_deleted, 0);
    }
    #[test]
    fn lock_file_layout_and_os_lock_exclusion() {
        let (temp, store, _, h) = fixture();
        let _guard = store.lock_object(&h).unwrap();
        let file = File::options()
            .read(true)
            .write(true)
            .open(temp.path().join(format!(".jmclcore/locks/sha1/{h}.lock")))
            .unwrap();
        assert!(file.try_lock().is_err());
        drop(_guard);
        file.try_lock().unwrap();
    }
    #[test]
    fn existing_lock_leaf_is_preserved_and_aliases_are_rejected() {
        let (temp, store, _, hash) = fixture();
        let leaf = temp
            .path()
            .join(format!(".jmclcore/locks/sha1/{hash}.lock"));
        std::fs::write(&leaf, b"existing lock bytes").unwrap();
        let before = leaf.metadata().unwrap();
        let guard = store.lock_object(&hash).unwrap();
        assert!(same(&before, &leaf.metadata().unwrap()));
        assert_eq!(std::fs::read(&leaf).unwrap(), b"existing lock bytes");
        drop(guard);
        std::fs::remove_file(&leaf).unwrap();
        let target = temp.path().join("external-lock");
        std::fs::write(&target, b"untouched").unwrap();
        symlink(&target, &leaf).unwrap();
        assert!(store.lock_object(&hash).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
        std::fs::remove_file(&leaf).unwrap();
        std::fs::create_dir(&leaf).unwrap();
        assert!(store.lock_object(&hash).is_err());
        assert!(leaf.is_dir());
    }
    #[test]
    fn concurrent_digest_transactions_create_nested_destinations() {
        use std::sync::{Barrier, atomic::AtomicUsize};
        // The content resolver can produce two different paths for one digest.
        // Keep their preflight and lock acquisition concurrent, and diagnose
        // each actual filesystem phase rather than serializing the fixture.
        for round in 0..32 {
            let temp = tempfile::tempdir().unwrap();
            let store = Store::open(temp.path().join("cas")).unwrap();
            let root = ArtifactRoot::open(temp.path().join("game")).unwrap();
            let hash = hash(b"content");
            let ready = Barrier::new(2);
            let downloads = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for name in ["mods/a.jar", "mods/dependency.jar"] {
                    let (store, root, hash, ready, downloads, temp) =
                        (&store, &root, &hash, &ready, &downloads, &temp);
                    scope.spawn(move || {
                        assert!(!root.verify(name, hash, 7).expect("destination preflight"));
                        ready.wait();
                        let tx = store.transaction(hash).unwrap_or_else(|error| {
                            eprintln!(
                                "round {round}, {name}, cache root {:?}, lock path {:?}",
                                store.root.metadata(),
                                std::fs::symlink_metadata(
                                    temp.path().join(format!("cas/locks/sha1/{hash}.lock"))
                                )
                            );
                            panic!("digest lock: {error:?}");
                        });
                        if !tx.verify(7).expect("locked cache verification") {
                            tx.remove_invalid(7).expect("remove invalid cache entry");
                            let mut staged = tx.stage(7).expect("create staging object");
                            staged
                                .file_mut()
                                .write_all(b"content")
                                .expect("write staged bytes");
                            staged.publish(false).expect("publish staged object");
                            downloads.fetch_add(1, Ordering::SeqCst);
                        }
                        assert_eq!(
                            tx.materialize_at(root, name, 7).unwrap_or_else(|e| panic!(
                                "round {round}, {name}: materialize: {e:?}"
                            )),
                            Placement::HardLink
                        );
                    });
                }
            });
            assert_eq!(downloads.load(Ordering::SeqCst), 1);
            let a = root.open_file("mods/a.jar").unwrap().metadata().unwrap();
            let b = root
                .open_file("mods/dependency.jar")
                .unwrap()
                .metadata()
                .unwrap();
            assert!(same(&a, &b));
            assert_eq!(a.nlink(), 3);
        }
    }
    #[test]
    fn fallback_errors_are_bounded() {
        assert!(fallback(rustix::io::Errno::XDEV));
        assert!(fallback(rustix::io::Errno::MLINK));
        assert!(!fallback(rustix::io::Errno::NOSPC));
        assert!(!fallback(rustix::io::Errno::IO));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cross_filesystem_copy_fallback_is_explicit_and_survives_gc() {
        let (temp, store, source, h) = fixture();
        let require = std::env::var_os("JMCL_REQUIRE_CROSS_DEVICE").is_some();
        let target = match tempfile::tempdir_in("/dev/shm") {
            Ok(dir) => dir,
            Err(error) => {
                assert!(!require, "cross-device fixture required: {error}");
                eprintln!("CROSS_DEVICE_NOT_EXERCISED: /dev/shm unavailable: {error}");
                return;
            }
        };
        if std::fs::metadata(temp.path()).unwrap().dev()
            == std::fs::metadata(target.path()).unwrap().dev()
        {
            assert!(!require, "cross-device fixture required but devices match");
            eprintln!("CROSS_DEVICE_NOT_EXERCISED: fixtures use the same device");
            return;
        }
        store.import_verified(&source, &h, 17).unwrap();
        let copied = target.path().join("copy");
        assert_eq!(store.materialize(&h, 17, &copied).unwrap(), Placement::Copy);
        assert_eq!(std::fs::read(&copied).unwrap(), b"verified artifact");
        std::fs::remove_file(source).unwrap();
        assert_eq!(
            store
                .collect(GcOptions {
                    dry_run: false,
                    min_age_seconds: 0
                })
                .unwrap()
                .objects_deleted,
            1
        );
        assert_eq!(std::fs::read(copied).unwrap(), b"verified artifact");
    }
    /// This explicit opt-in check runs an independently built, pinned Zig Core.
    #[test]
    #[ignore = "requires JMCL_ZIG_ORACLE pointing to pinned Zig Core"]
    fn zig_gc_summaries_and_digest_lock_are_interoperable() {
        use std::process::{Command, Stdio};
        let oracle = std::env::var_os("JMCL_ZIG_ORACLE").expect("set JMCL_ZIG_ORACLE");
        fn prepare(path: &Path) -> (Store, String) {
            let store = Store::open(path).unwrap();
            for (data, linked) in [
                (b"eligible".as_slice(), false),
                (b"linked".as_slice(), true),
            ] {
                let h = hash(data);
                let object = path.join(object_path(&h).unwrap());
                std::fs::create_dir_all(object.parent().unwrap()).unwrap();
                std::fs::write(&object, data).unwrap();
                if linked {
                    std::fs::hard_link(&object, path.join("instance-link")).unwrap();
                }
            }
            std::fs::write(path.join("objects/sha1/junk"), b"ignored").unwrap();
            (store, hash(b"eligible"))
        }
        let temp = tempfile::tempdir().unwrap();
        let zig_path = temp.path().join("zig");
        let rust_path = temp.path().join("rust");
        let (_, _) = prepare(&zig_path);
        let (rust, _) = prepare(&rust_path);
        for dry_run in [true, false] {
            let mut command = Command::new(&oracle);
            command
                .args(["store", "gc", "--store-directory"])
                .arg(&zig_path)
                .args(["--min-age-seconds", "0", "--json"]);
            if !dry_run {
                command.arg("--execute");
            }
            let output = command.output().unwrap();
            assert!(output.status.success());
            let zig: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            let summary = rust
                .collect(GcOptions {
                    dry_run,
                    min_age_seconds: 0,
                })
                .unwrap();
            assert_eq!(
                zig["result"]["summary"],
                serde_json::to_value(summary).unwrap()
            );
        }
        let shared = temp.path().join("shared");
        let (store, h) = prepare(&shared);
        let guard = store.lock_object(&h).unwrap();
        let mut child = Command::new(&oracle)
            .args(["store", "gc", "--store-directory"])
            .arg(&shared)
            .args(["--min-age-seconds", "0", "--execute", "--json"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(child.try_wait().unwrap().is_none());
        assert!(shared.join(object_path(&h).unwrap()).exists());
        drop(guard);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Zig GC did not finish after the shared digest lock was released");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let zig: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(zig["result"]["summary"]["objects_deleted"], 1);
        assert!(!shared.join(object_path(&h).unwrap()).exists());
    }
}

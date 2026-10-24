use super::*;
use rustix::fs::{self as rfs, AtFlags, Mode, OFlags};
use sha1::{Digest, Sha1};
use std::fs::{File, Metadata, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

pub struct Store {
    root: File,
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
fn dir_at(parent: &File, name: &std::ffi::OsStr, create: bool) -> io::Result<File> {
    if create {
        match rfs::mkdirat(parent, name, Mode::from_raw_mode(0o755)) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(File::from(cv(rfs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ))?))
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
        Ok(Self { root })
    }
    pub fn open_existing(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: walk(path.as_ref(), false)?,
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
        let dir = dir_at(
            &dir_at(&self.root, "locks".as_ref(), true)?,
            "sha1".as_ref(),
            true,
        )?;
        let name = format!("{hash}.lock");
        let file = File::from(cv(rfs::openat(
            &dir,
            name.as_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        ))?);
        if !file.metadata()?.is_file() {
            return Err(invalid("InvalidLockKind"));
        }
        file.lock()?;
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
        let mut source = file_at(&source_dir, source_name)?;
        if !matches(&mut source, &hash, size)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ArtifactVerificationFailed",
            ));
        }
        let prefix = self.prefix(&hash, true)?;
        if self.verify(&hash, size)? {
            return Ok(Placement::HardLink);
        }
        // Repair publishes a distinct inode: never overwrite a shared object in place.
        let temp = format!(".jmcl-object-{}", unique());
        let placement = match rfs::linkat(
            &source_dir,
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
            if !matches(&mut linked, &hash, size)? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ArtifactVerificationFailed",
                ));
            }
            readonly(&linked)?;
            cv(rfs::renameat(
                &prefix,
                temp.as_str(),
                &prefix,
                hash.as_str(),
            ))
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
        let prefix = self.prefix(&hash, false)?;
        let mut source = file_at(&prefix, hash.as_ref())?;
        if !matches(&mut source, &hash, size)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ArtifactVerificationFailed",
            ));
        }
        readonly(&source)?;
        let (dest_dir, dest_name) = split_file(destination.as_ref(), true)?;
        let temp = format!("{}.jmcl-link-{}", dest_name.to_string_lossy(), unique());
        match rfs::linkat(
            &prefix,
            hash.as_str(),
            &dest_dir,
            temp.as_str(),
            AtFlags::empty(),
        ) {
            Ok(()) => {
                let finish = (|| {
                    if let Ok(existing) = file_at(&dest_dir, dest_name)
                        && same(&existing.metadata()?, &source.metadata()?)
                    {
                        return Ok(());
                    }
                    cv(rfs::renameat(
                        &dest_dir,
                        temp.as_str(),
                        &dest_dir,
                        dest_name,
                    ))
                })();
                let _ = rfs::unlinkat(&dest_dir, temp.as_str(), AtFlags::empty());
                finish?;
                Ok(Placement::HardLink)
            }
            Err(e) if fallback(e) => {
                // Match Zig's documented non-atomic copy fallback, without following
                // final-component symlinks or truncating a shared inode.
                let mut output = match rfs::openat(
                    &dest_dir,
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
                        let existing = file_at(&dest_dir, dest_name)?;
                        if matches(&mut existing.try_clone()?, &hash, size)? {
                            return Ok(Placement::Copy);
                        }
                        cv(rfs::unlinkat(&dest_dir, dest_name, AtFlags::empty()))?;
                        File::from(cv(rfs::openat(
                            &dest_dir,
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

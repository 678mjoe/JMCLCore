//! Windows CAS: single-component NT operations relative to validated handles.
//! Reparse points are rejected, immutable replacement never changes an old
//! shared inode, and GC unlinks read-only objects without clearing their flags.
use super::*;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem as nt;
use windows_sys::Win32::Foundation::{
    HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

pub struct Store {
    root: File,
}
pub struct ObjectLock {
    _file: File,
}
fn invalid(s: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
fn status(s: i32) -> io::Result<()> {
    if s < 0 {
        Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(s) } as i32,
        ))
    } else {
        Ok(())
    }
}
fn leaf(name: &std::ffi::OsStr) -> io::Result<Vec<u16>> {
    let wide: Vec<u16> = name.encode_wide().collect();
    if wide.is_empty()
        || wide.len() > 32766
        || wide
            .iter()
            .any(|c| *c == 0 || *c == b'/' as u16 || *c == b'\\' as u16 || *c == b':' as u16)
        || wide == [46]
        || wide == [46, 46]
    {
        return Err(invalid("InvalidFileName"));
    }
    Ok(wide)
}
fn info(f: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut v = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut v) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if v.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || unsafe { GetFileType(f.as_raw_handle()) } != FILE_TYPE_DISK
    {
        return Err(invalid("ReparseOrNonDiskObject"));
    }
    Ok(v)
}
fn open_nt_raw(
    parent: Option<&File>,
    wide: &mut [u16],
    directory: Option<bool>,
    disposition: u32,
    write: bool,
    mutation: bool,
) -> io::Result<File> {
    open_nt_raw_policy(parent, wide, directory, disposition, write, mutation, false)
}
#[allow(clippy::too_many_arguments)]
fn open_nt_raw_policy(
    parent: Option<&File>,
    wide: &mut [u16],
    directory: Option<bool>,
    disposition: u32,
    write: bool,
    mutation: bool,
    follow_leaf: bool,
) -> io::Result<File> {
    let name = UNICODE_STRING {
        Length: (wide.len() * 2) as u16,
        MaximumLength: (wide.len() * 2) as u16,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.map_or(null_mut(), |p| p.as_raw_handle()),
        ObjectName: &name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: null(),
        SecurityQualityOfService: null(),
    };
    let mut handle: HANDLE = null_mut();
    let mut ios = IO_STATUS_BLOCK::default();
    let access = FILE_GENERIC_READ
        | if mutation {
            DELETE | FILE_WRITE_ATTRIBUTES
        } else {
            0
        }
        | if write { FILE_GENERIC_WRITE } else { 0 };
    let share = FILE_SHARE_READ
        | if directory == Some(true) || write {
            FILE_SHARE_WRITE
        } else {
            0
        }
        | if directory != Some(true) || mutation {
            FILE_SHARE_DELETE
        } else {
            0
        };
    // Synchronous handles match std::fs::File's Read/Write contract. OPEN_REPARSE
    // opens the link itself; checked handle metadata rejects it before use.
    status(unsafe {
        nt::NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut ios,
            null(),
            FILE_ATTRIBUTE_NORMAL,
            share,
            disposition,
            (if follow_leaf {
                0
            } else {
                nt::FILE_OPEN_REPARSE_POINT
            }) | nt::FILE_SYNCHRONOUS_IO_NONALERT
                | match directory {
                    Some(true) => nt::FILE_DIRECTORY_FILE,
                    Some(false) => nt::FILE_NON_DIRECTORY_FILE,
                    None => 0,
                },
            null(),
            0,
        )
    })?;
    Ok(unsafe { File::from_raw_handle(handle) })
}
fn open_nt(
    parent: Option<&File>,
    wide: &mut [u16],
    directory: bool,
    disposition: u32,
    write: bool,
) -> io::Result<File> {
    let file = open_nt_raw(
        parent,
        wide,
        Some(directory),
        disposition,
        write,
        !directory,
    )?;
    let metadata = info(&file)?;
    if (metadata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory {
        return Err(invalid("InvalidObjectKind"));
    }
    Ok(file)
}
/// Read-only regular-file open. Loading a public manifest does not require
/// DELETE or WRITE_ATTRIBUTES permissions.
pub(crate) fn read_at(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    let file = open_nt_raw(
        Some(dir),
        &mut leaf(name)?,
        Some(false),
        nt::FILE_OPEN,
        false,
        false,
    )?;
    info(&file)?;
    Ok(file)
}
/// Compatibility-only read alias: the pinned Core follows instance.json links.
/// Single leaf, read-only access, pinned parent handle, regular disk-file target.
/// Never use this for CAS writes, immutable artifacts or mutable save operations.
pub(crate) fn read_follow_at(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    let file = open_nt_raw_policy(
        Some(dir),
        &mut leaf(name)?,
        Some(false),
        nt::FILE_OPEN,
        false,
        false,
        true,
    )?;
    info(&file)?;
    Ok(file)
}
/// Compatibility directory read: original content scans follow named directory
/// aliases. The resolved directory handle anchors subsequent child operations;
/// CAS publication continues using the no-follow `child` API.
pub(crate) fn read_dir_follow_at(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    let file = open_nt_raw_policy(
        Some(dir),
        &mut leaf(name)?,
        Some(true),
        nt::FILE_OPEN,
        false,
        false,
        true,
    )?;
    let metadata = info(&file)?;
    if metadata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(invalid("InvalidObjectKind"));
    }
    Ok(file)
}
pub(crate) fn child(
    dir: &File,
    name: &std::ffi::OsStr,
    directory: bool,
    create: bool,
    write: bool,
) -> io::Result<File> {
    open_nt(
        Some(dir),
        &mut leaf(name)?,
        directory,
        if create {
            nt::FILE_OPEN_IF
        } else {
            nt::FILE_OPEN
        },
        write,
    )
}
pub(crate) fn create_file(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    open_nt(Some(dir), &mut leaf(name)?, false, nt::FILE_CREATE, true)
}
pub(crate) fn create_dir(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    open_nt(Some(dir), &mut leaf(name)?, true, nt::FILE_CREATE, false)
}
/// A caller-supplied root is a path boundary: like Zig, ancestors may resolve
/// through a junction, but the final root itself may not be a reparse point.
/// All names beneath it use validated handle-relative opens.
pub(crate) fn walk(path: &Path, create: bool) -> io::Result<File> {
    if path.as_os_str().is_empty() {
        return Err(invalid("EmptyStoreDirectory"));
    }
    if create {
        std::fs::create_dir_all(path)?;
    }
    let absolute = std::path::absolute(path)?;
    let text = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    // std::path::absolute uses Win32 full-path resolution. Map its DOS/UNC
    // namespace to the NT namespace without canonicalizing the final link.
    let slash = b'\\' as u16;
    let mut wide = if text.starts_with(&[slash, slash, b'?' as u16, slash]) {
        let mut prefix: Vec<u16> = r"\??\".encode_utf16().collect();
        prefix.extend_from_slice(&text[4..]);
        prefix
    } else if text.starts_with(&[slash, slash]) {
        let mut prefix: Vec<u16> = r"\??\UNC\".encode_utf16().collect();
        prefix.extend_from_slice(&text[2..]);
        prefix
    } else {
        let mut prefix: Vec<u16> = r"\??\".encode_utf16().collect();
        prefix.extend_from_slice(&text);
        prefix
    };
    if wide.len() > 32766 || wide.contains(&0) {
        return Err(invalid("InvalidRootPath"));
    }
    open_nt(None, &mut wide, true, nt::FILE_OPEN, false)
}
fn split(path: &Path, create: bool) -> io::Result<(File, &std::ffi::OsStr)> {
    let name = path.file_name().ok_or_else(|| invalid("InvalidFilePath"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((walk(parent, create)?, name))
}
pub(crate) fn file_at(dir: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    child(dir, name, false, false, false)
}
fn digest(file: &mut File) -> io::Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut h = Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        h.update(&buffer[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn matches(file: &mut File, hash: &str, size: u64) -> io::Result<bool> {
    Ok(file.metadata()?.len() == size && digest(file)?.eq_ignore_ascii_case(hash))
}
fn same(a: &File, b: &File) -> io::Result<bool> {
    let a = info(a)?;
    let b = info(b)?;
    Ok(a.dwVolumeSerialNumber == b.dwVolumeSerialNumber
        && a.nFileIndexHigh == b.nFileIndexHigh
        && a.nFileIndexLow == b.nFileIndexLow)
}
fn readonly(f: &File) -> io::Result<()> {
    let mut basic = FILE_BASIC_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            f.as_raw_handle(),
            FileBasicInfo,
            &mut basic as *mut _ as _,
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    basic.FileAttributes |= FILE_ATTRIBUTE_READONLY;
    if unsafe {
        SetFileInformationByHandle(
            f.as_raw_handle(),
            FileBasicInfo,
            &basic as *const _ as _,
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) fn unlink(f: &File) -> io::Result<()> {
    let mut value = nt::FILE_DISPOSITION_INFORMATION_EX {
        Flags: nt::FILE_DISPOSITION_DELETE
            | nt::FILE_DISPOSITION_POSIX_SEMANTICS
            | nt::FILE_DISPOSITION_IGNORE_READONLY_ATTRIBUTE,
    };
    let mut ios = IO_STATUS_BLOCK::default();
    // Never clear READONLY on the inode: other instance links must stay immutable.
    status(unsafe {
        nt::NtSetInformationFile(
            f.as_raw_handle(),
            &mut ios,
            &mut value as *mut _ as _,
            size_of::<nt::FILE_DISPOSITION_INFORMATION_EX>() as u32,
            nt::FileDispositionInformationEx,
        )
    })
}
pub(crate) fn named_op(
    source: &File,
    dest: &File,
    name: &std::ffi::OsStr,
    rename: bool,
) -> io::Result<()> {
    let wide = leaf(name)?;
    // The shared storage must obey both native ABI layouts, including trailing
    // structure padding for short leaf names (not just FileNameLength bytes).
    const {
        assert!(size_of::<nt::FILE_LINK_INFORMATION>() == size_of::<nt::FILE_RENAME_INFORMATION>());
        assert!(
            offset_of!(nt::FILE_LINK_INFORMATION, FileName)
                == offset_of!(nt::FILE_RENAME_INFORMATION, FileName)
        );
        assert!(
            offset_of!(nt::FILE_LINK_INFORMATION, RootDirectory)
                == offset_of!(nt::FILE_RENAME_INFORMATION, RootDirectory)
        );
        assert!(
            offset_of!(nt::FILE_LINK_INFORMATION, FileNameLength)
                == offset_of!(nt::FILE_RENAME_INFORMATION, FileNameLength)
        );
    }
    // Allocate u64 words for alignment.
    let offset = offset_of!(nt::FILE_LINK_INFORMATION, FileName);
    let bytes = (offset + wide.len() * 2).max(size_of::<nt::FILE_LINK_INFORMATION>());
    let mut buffer = vec![0u64; bytes.div_ceil(8)];
    let p = buffer.as_mut_ptr().cast::<nt::FILE_LINK_INFORMATION>();
    unsafe {
        (*p).Anonymous.Flags = if rename {
            nt::FILE_RENAME_REPLACE_IF_EXISTS
                | nt::FILE_RENAME_POSIX_SEMANTICS
                | nt::FILE_RENAME_IGNORE_READONLY_ATTRIBUTE
        } else {
            0
        };
        (*p).RootDirectory = dest.as_raw_handle();
        (*p).FileNameLength = (wide.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), (*p).FileName.as_mut_ptr(), wide.len());
    }
    let mut ios = IO_STATUS_BLOCK::default();
    status(unsafe {
        nt::NtSetInformationFile(
            source.as_raw_handle(),
            &mut ios,
            p.cast(),
            bytes as u32,
            if rename {
                nt::FileRenameInformationEx
            } else {
                nt::FileLinkInformation
            },
        )
    })
}
fn fallback(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5 | 17 | 50 | 1142))
}
fn unique() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}
fn copy(source: &mut File, output: &mut File) -> io::Result<()> {
    source.seek(SeekFrom::Start(0))?;
    io::copy(source, output)?;
    output.sync_all()
}
pub(crate) fn entries(dir: &File) -> io::Result<Vec<(std::ffi::OsString, u32)>> {
    let mut all = Vec::new();
    let mut restart = true;
    loop {
        let mut buffer = vec![0u64; 8192];
        let mut ios = IO_STATUS_BLOCK::default();
        let result = unsafe {
            nt::NtQueryDirectoryFile(
                dir.as_raw_handle(),
                null_mut(),
                None,
                null(),
                &mut ios,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                nt::FileDirectoryInformation,
                false,
                null(),
                restart,
            )
        };
        restart = false;
        if result == 0x80000006u32 as i32 {
            break;
        }
        status(result)?;
        let count = ios.Information;
        if count > buffer.len() * 8 {
            return Err(invalid("DirectoryRecordInvalid"));
        }
        let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), count) };
        let mut at = 0usize;
        loop {
            if at + 64 > bytes.len() {
                return Err(invalid("DirectoryRecordInvalid"));
            }
            let next = u32::from_ne_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let length = u32::from_ne_bytes(bytes[at + 60..at + 64].try_into().unwrap()) as usize;
            if !length.is_multiple_of(2) || at + 64 + length > bytes.len() {
                return Err(invalid("DirectoryRecordInvalid"));
            }
            let attributes = u32::from_ne_bytes(bytes[at + 56..at + 60].try_into().unwrap());
            let wide: Vec<u16> = bytes[at + 64..at + 64 + length]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_ne_bytes([b[0], b[1]]))
                .collect();
            use std::os::windows::ffi::OsStringExt;
            let name = std::ffi::OsString::from_wide(&wide);
            if name != "." && name != ".." {
                all.push((name, attributes));
            }
            if next == 0 {
                break;
            }
            if next < 64 || !next.is_multiple_of(4) {
                return Err(invalid("DirectoryRecordInvalid"));
            }
            at += next;
        }
    }
    Ok(all)
}
pub(crate) fn names(dir: &File) -> io::Result<Vec<std::ffi::OsString>> {
    Ok(entries(dir)?.into_iter().map(|(name, _)| name).collect())
}
/// Deletes an owned subtree by handles. Top-level reparse points are rejected;
/// inner reparse entries are unlinked themselves, never traversed.
pub(crate) fn remove_tree(parent: &File, name: &std::ffi::OsStr) -> io::Result<()> {
    remove_tree_inner(parent, name, true)
}
fn remove_tree_inner(parent: &File, name: &std::ffi::OsStr, top: bool) -> io::Result<()> {
    let file = open_nt_raw(
        Some(parent),
        &mut leaf(name)?,
        None,
        nt::FILE_OPEN,
        false,
        true,
    )?;
    let mut metadata = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut metadata) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { GetFileType(file.as_raw_handle()) } != FILE_TYPE_DISK {
        return Err(invalid("InvalidObjectKind"));
    }
    if metadata.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        if top {
            return Err(invalid("ReparseObject"));
        }
        return unlink(&file);
    }
    if metadata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        for child in names(&file)? {
            remove_tree_inner(&file, &child, false)?;
        }
    }
    unlink(&file)
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let root = walk(path.as_ref(), true)?;
        for name in ["objects", "locks"] {
            let dir = child(&root, name.as_ref(), true, true, false)?;
            child(&dir, "sha1".as_ref(), true, true, false)?;
        }
        Ok(Self { root })
    }
    pub fn open_existing(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: walk(path.as_ref(), false)?,
        })
    }
    fn objects(&self) -> io::Result<File> {
        child(
            &child(&self.root, "objects".as_ref(), true, false, false)?,
            "sha1".as_ref(),
            true,
            false,
            false,
        )
    }
    fn prefix(&self, hash: &str, create: bool) -> io::Result<File> {
        child(&self.objects()?, hash[..2].as_ref(), true, create, false)
    }
    pub fn lock_object(&self, hash: &str) -> io::Result<ObjectLock> {
        let hash = normalized_hash(hash)?;
        let dir = child(
            &child(&self.root, "locks".as_ref(), true, true, false)?,
            "sha1".as_ref(),
            true,
            true,
            false,
        )?;
        let file = child(&dir, format!("{hash}.lock").as_ref(), false, true, true)?;
        file.lock()?;
        Ok(ObjectLock { _file: file })
    }
    pub fn verify(&self, hash: &str, size: u64) -> io::Result<bool> {
        let hash = normalized_hash(hash)?;
        let result = (|| {
            matches(
                &mut file_at(&self.prefix(&hash, false)?, hash.as_ref())?,
                &hash,
                size,
            )
        })();
        match result {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            v => v,
        }
    }
    pub fn import_verified(
        &self,
        source: impl AsRef<Path>,
        hash: &str,
        size: u64,
    ) -> io::Result<Placement> {
        let hash = normalized_hash(hash)?;
        let _lock = self.lock_object(&hash)?;
        let (source_dir, source_name) = split(source.as_ref(), false)?;
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
        let temp = format!(".jmcl-object-{}", unique());
        let placement = match named_op(&source, &prefix, temp.as_ref(), false) {
            Ok(()) => Placement::HardLink,
            Err(e) if fallback(&e) => {
                let mut output = create_file(&prefix, temp.as_ref())?;
                if let Err(e) = copy(&mut source, &mut output) {
                    let _ = unlink(&output);
                    return Err(e);
                }
                Placement::Copy
            }
            Err(e) => return Err(e),
        };
        let mut object = file_at(&prefix, temp.as_ref())?;
        let publish = (|| {
            if !matches(&mut object, &hash, size)? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ArtifactVerificationFailed",
                ));
            }
            readonly(&object)?;
            named_op(&object, &prefix, hash.as_ref(), true)
        })();
        if publish.is_err() {
            let _ = unlink(&object);
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
        let (dest, name) = split(destination.as_ref(), true)?;
        match file_at(&dest, name) {
            Ok(mut existing) => {
                if matches(&mut existing, &hash, size)? {
                    return Ok(if same(&source, &existing)? {
                        Placement::HardLink
                    } else {
                        Placement::Copy
                    });
                }
                unlink(&existing)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        match named_op(&source, &dest, name, false) {
            Ok(()) => Ok(Placement::HardLink),
            Err(e) if fallback(&e) => {
                let mut output = create_file(&dest, name)?;
                copy(&mut source, &mut output)?;
                readonly(&output)?;
                Ok(Placement::Copy)
            }
            Err(e) => Err(e),
        }
    }
    pub fn collect(&self, options: GcOptions) -> io::Result<GcSummary> {
        let objects = match self.objects() {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(GcSummary::default()),
            Err(e) => return Err(e),
        };
        let mut s = GcSummary::default();
        for prefix_name in names(&objects)? {
            let Some(p) = prefix_name
                .to_str()
                .filter(|v| v.len() == 2 && lower_hex(v))
            else {
                s.objects_ignored += 1;
                continue;
            };
            let prefix = match child(&objects, p.as_ref(), true, false, false) {
                Ok(v) => v,
                Err(_) => {
                    s.objects_ignored += 1;
                    continue;
                }
            };
            for name in names(&prefix)? {
                let Some(hash) = name
                    .to_str()
                    .filter(|v| v.len() == 40 && v.starts_with(p) && lower_hex(v))
                else {
                    s.objects_ignored += 1;
                    continue;
                };
                let file = match file_at(&prefix, &name) {
                    Ok(f) => f,
                    Err(_) => {
                        s.objects_ignored += 1;
                        continue;
                    }
                };
                s.objects_scanned += 1;
                let result = (|| {
                    if !eligible(&file, options, &mut s)? {
                        return Ok(());
                    }
                    if options.dry_run {
                        s.objects_eligible += 1;
                        s.bytes_eligible += file.metadata()?.len();
                        return Ok(());
                    }
                    drop(file);
                    let _lock = self.lock_object(hash)?;
                    let current = file_at(&prefix, &name)?;
                    if !eligible(&current, options, &mut s)? {
                        return Ok(());
                    }
                    let size = current.metadata()?.len();
                    s.objects_eligible += 1;
                    s.bytes_eligible += size;
                    unlink(&current)?;
                    s.objects_deleted += 1;
                    s.bytes_deleted += size;
                    Ok::<_, io::Error>(())
                })();
                if let Err(e) = result {
                    if e.kind() == io::ErrorKind::NotFound || e.kind() == io::ErrorKind::WouldBlock
                    {
                        s.objects_busy += 1;
                    } else {
                        s.errors += 1;
                    }
                }
            }
        }
        Ok(s)
    }
}
fn lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn eligible(f: &File, options: GcOptions, s: &mut GcSummary) -> io::Result<bool> {
    if info(f)?.nNumberOfLinks != 1 {
        s.objects_linked += 1;
        return Ok(false);
    }
    if !SystemTime::now()
        .duration_since(f.metadata()?.modified()?)
        .map(|d| d >= Duration::from_secs(options.min_age_seconds))
        .unwrap_or(false)
    {
        s.objects_young += 1;
        return Ok(false);
    }
    Ok(true)
}

/// Rename a validated entry using mutation rights without reopening its path.
pub(crate) fn rename_at(
    parent: &File,
    old: &std::ffi::OsStr,
    dest: &File,
    new: &std::ffi::OsStr,
) -> io::Result<()> {
    let file = open_nt_raw(
        Some(parent),
        &mut leaf(old)?,
        None,
        nt::FILE_OPEN,
        false,
        true,
    )?;
    info(&file)?;
    named_op(&file, dest, new, true)
}
/// Match case-insensitive NT entry lookup without following the entry itself.
pub(crate) fn entry_exists(parent: &File, name: &std::ffi::OsStr) -> bool {
    let Ok(mut wide) = leaf(name) else {
        return false;
    };
    open_nt_raw(Some(parent), &mut wide, None, nt::FILE_OPEN, false, false).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn hardlinks_share_identity_and_are_readonly() {
        let (temp, store, source, h) = fixture();
        assert_eq!(
            store.import_verified(&source, &h, 17).unwrap(),
            Placement::HardLink
        );
        let dest = temp.path().join("instance/libraries/artifact.jar");
        assert_eq!(
            store.materialize(&h, 17, &dest).unwrap(),
            Placement::HardLink
        );
        let object = store.prefix(&h, false).unwrap();
        let object = file_at(&object, h.as_ref()).unwrap();
        let (parent, name) = split(&dest, false).unwrap();
        let target = file_at(&parent, name).unwrap();
        assert!(same(&object, &target).unwrap());
        assert_eq!(info(&target).unwrap().nNumberOfLinks, 3);
        assert_ne!(
            info(&target).unwrap().dwFileAttributes & FILE_ATTRIBUTE_READONLY,
            0
        );
        assert!(store.verify(&h, 17).unwrap());
    }
    #[test]
    fn corrupt_source_never_publishes() {
        let (_temp, store, source, h) = fixture();
        assert!(store.import_verified(&source, &h, 18).is_err());
        assert!(!store.verify(&h, 17).unwrap());
        assert!(store.import_verified(&source, &"0".repeat(40), 17).is_err());
    }
    #[test]
    fn replacing_destination_preserves_old_shared_inode() {
        let (temp, store, source, h) = fixture();
        store.import_verified(&source, &h, 17).unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        store.materialize(&h, 17, &a).unwrap();
        store.materialize(&h, 17, &b).unwrap();
        let source2 = temp.path().join("new");
        std::fs::write(&source2, b"replacement").unwrap();
        let h2 = hash(b"replacement");
        store.import_verified(&source2, &h2, 11).unwrap();
        store.materialize(&h2, 11, &a).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"replacement");
        assert_eq!(std::fs::read(&b).unwrap(), b"verified artifact");
        assert!(std::fs::metadata(&b).unwrap().permissions().readonly());
        assert!(store.verify(&h, 17).unwrap());
    }
    #[test]
    fn gc_dry_run_and_execute_unlink_without_mutating_other_links() {
        let (temp, store, source, h) = fixture();
        store.import_verified(&source, &h, 17).unwrap();
        let linked = store
            .collect(GcOptions {
                dry_run: false,
                min_age_seconds: 0,
            })
            .unwrap();
        assert_eq!(linked.objects_linked, 1);
        assert_eq!(linked.objects_deleted, 0);
        let (parent, name) = split(&source, false).unwrap();
        let file = file_at(&parent, name).unwrap();
        unlink(&file).unwrap();
        drop(file);
        drop(parent);
        let preview = store
            .collect(GcOptions {
                dry_run: true,
                min_age_seconds: 0,
            })
            .unwrap();
        assert_eq!(preview.objects_eligible, 1);
        assert_eq!(preview.bytes_eligible, 17);
        assert_eq!(preview.objects_deleted, 0);
        assert!(store.verify(&h, 17).unwrap());
        let removed = store
            .collect(GcOptions {
                dry_run: false,
                min_age_seconds: 0,
            })
            .unwrap();
        assert_eq!(removed.objects_deleted, 1);
        assert_eq!(removed.bytes_deleted, 17);
        assert!(!store.verify(&h, 17).unwrap());
        assert!(
            temp.path()
                .join(".jmclcore/locks/sha1")
                .join(format!("{h}.lock"))
                .is_file()
        );
    }
    #[test]
    fn store_and_destination_junctions_are_rejected() {
        let (temp, store, source, h) = fixture();
        store.import_verified(&source, &h, 17).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let junction = temp.path().join("junction");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        assert!(Store::open(&junction).is_err());
        assert!(store.materialize(&h, 17, junction.join("escape")).is_err());
        assert!(!outside.join("escape").exists());
        std::fs::remove_dir(&junction).unwrap();
    }
    #[test]
    fn object_lock_interoperates_with_independent_file_handle() {
        let (temp, store, _source, h) = fixture();
        let lock = store.lock_object(&h).unwrap();
        let path = temp
            .path()
            .join(".jmclcore/locks/sha1")
            .join(format!("{h}.lock"));
        let second = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(second.try_lock().is_err());
        drop(lock);
        second.try_lock().unwrap();
    }
    #[test]
    fn valid_existing_copy_remains_copy_and_exact_disk_path() {
        let (temp, store, source, h) = fixture();
        store.import_verified(&source, &h, 17).unwrap();
        let dest = temp.path().join("existing");
        std::fs::write(&dest, b"verified artifact").unwrap();
        assert_eq!(store.materialize(&h, 17, &dest).unwrap(), Placement::Copy);
        assert!(
            temp.path()
                .join(".jmclcore")
                .join(object_path(&h).unwrap())
                .is_file()
        );
    }
    #[test]
    fn configured_root_may_have_junction_ancestor_but_not_junction_leaf() {
        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("actual");
        std::fs::create_dir(&actual).unwrap();
        let link = temp.path().join("link");
        let out = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&actual)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "junction creation failed: {:?}; {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(Store::open(&link).is_err());
        let store = Store::open(link.join("nested")).unwrap();
        assert_eq!(
            store.collect(GcOptions::default()).unwrap(),
            GcSummary::default()
        );
        drop(store);
        assert!(actual.join("nested/objects/sha1").is_dir());
        std::fs::remove_dir(&link).unwrap();
    }
    #[test]
    fn shared_helpers_typed_entries_and_anchored_delete_do_not_follow_junctions() {
        let temp = tempfile::tempdir().unwrap();
        let root = walk(temp.path(), false).unwrap();
        let dir = create_dir(&root, "instance".as_ref()).unwrap();
        assert_eq!(
            create_dir(&root, "instance".as_ref()).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let mut file = create_file(&dir, "manifest.json".as_ref()).unwrap();
        use std::io::Write;
        file.write_all(b"public").unwrap();
        drop(file);
        let mut read = read_at(&dir, "manifest.json".as_ref()).unwrap();
        let mut bytes = Vec::new();
        read.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"public");
        drop(read);
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep"), b"keep").unwrap();
        let junction = temp.path().join("instance").join("junction");
        let out = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "junction creation failed: {:?}; {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let found = entries(&dir).unwrap();
        assert!(found.iter().any(|(n, a)| n == "manifest.json"
            && a & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0));
        assert!(
            found
                .iter()
                .any(|(n, a)| n == "junction" && a & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        );
        drop(dir);
        remove_tree(&root, "instance".as_ref()).unwrap();
        assert!(!temp.path().join("instance").exists());
        assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"keep");
    }
    #[test]
    fn fallback_error_set_is_explicit() {
        for code in [5, 17, 50, 1142] {
            assert!(fallback(&io::Error::from_raw_os_error(code)));
        }
        for code in [2, 3, 32, 87, 112] {
            assert!(!fallback(&io::Error::from_raw_os_error(code)));
        }
    }
}

use super::fs::FileType;
use super::*;
use std::{
    fs::File,
    io::Cursor,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
fn path_valid(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 4096
        || name.starts_with('/')
        || name.bytes().any(|b| b == 0 || b == b'\\' || b == b':')
        || name.split('/').any(|p| p == "..")
    {
        return Err(Error::ArchiveInvalid);
    }
    Ok(())
}
fn ziperr(_: zip::result::ZipError) -> Error {
    Error::ArchiveInvalid
}
fn datetime(t: SystemTime) -> zip::DateTime {
    let sec = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(315532800)
        .max(315532800);
    let t =
        time::OffsetDateTime::from_unix_timestamp(sec).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    zip::DateTime::from_date_and_time(
        t.year().clamp(1980, 2107) as u16,
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    )
    .unwrap_or_default()
}
fn add_dir(writer: &mut ZipWriter<File>, d: &fs::Dir, prefix: &str, files: &mut u32) -> Result<()> {
    for (name, kind) in d.names()? {
        let name = name.to_str().ok_or(Error::UnsupportedEntry)?;
        if (kind == FileType::RegularFile && name == "session.lock")
            || name == ".jmcl"
            || is_temp(name)
        {
            continue;
        }
        let path = format!("{prefix}/{name}");
        if path.len() > 4096 {
            return Err(Error::ArchiveTooLarge);
        }
        match kind {
            FileType::RegularFile => {
                let mut source = d.file(name, false)?;
                let m = source.metadata()?;
                if m.len() > u32::MAX as u64 {
                    return Err(Error::ArchiveTooLarge);
                }
                let options = SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .last_modified_time(datetime(m.modified()?));
                writer.start_file(path, options).map_err(ziperr)?;
                std::io::copy(&mut source, writer)?;
                *files += 1;
            }
            FileType::Directory => {
                let child = d.child(name, false)?;
                let options = SimpleFileOptions::default()
                    .last_modified_time(datetime(child.0.metadata()?.modified()?));
                writer
                    .add_directory(format!("{path}/"), options)
                    .map_err(ziperr)?;
                add_dir(writer, &child, &path, files)?;
            }
            _ => return Err(Error::UnsupportedEntry),
        }
    }
    Ok(())
}
fn export_to(saves: &fs::Dir, name: &str, parent: &fs::Dir, leaf: &str) -> Result<(u64, u32)> {
    let d = guard(saves, name)?;
    let tmp = unique(".jmcl-export-", leaf)?;
    let result = (|| {
        let file = parent.new_file(&tmp)?;
        let mut writer = ZipWriter::new(file);
        let mut files = 0;
        add_dir(&mut writer, &d, name, &mut files)?;
        let file = writer.finish().map_err(ziperr)?;
        file.sync_all()?;
        let size = file.metadata()?.len();
        if size > u32::MAX as u64 {
            return Err(Error::ArchiveTooLarge);
        }
        drop(file);
        parent.rename(&tmp, parent, leaf)?;
        Ok((size, files))
    })();
    if result.is_err() {
        let _ = parent.unlink(&tmp);
    }
    result
}
pub fn export(path: &str, name: &str, file: &str) -> Result<Value> {
    let dest = Path::new(file);
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "missing destination parent",
        )));
    }
    let d = fs::Dir::root(parent)?;
    let leaf = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(Error::InvalidName)?;
    let (size, files) = export_to(&saves(path)?, name, &d, leaf)?;
    Ok(json!({"stage":"worlds.export","file":file,"size_bytes":size,"files":files}))
}
fn archive(f: File) -> Result<ZipArchive<Cursor<Vec<u8>>>> {
    let mut data = vec![];
    f.take(MAX_ARCHIVE as u64 + 1).read_to_end(&mut data)?;
    if data.len() > MAX_ARCHIVE {
        return Err(Error::ArchiveTooLarge);
    }
    let end = data
        .windows(4)
        .rposition(|s| s == b"PK\x05\x06")
        .ok_or(Error::ArchiveInvalid)?;
    let e = data.get(end..end + 22).ok_or(Error::ArchiveInvalid)?;
    let count = u16::from_le_bytes([e[10], e[11]]);
    let size = u32::from_le_bytes(e[12..16].try_into().unwrap());
    let off = u32::from_le_bytes(e[16..20].try_into().unwrap());
    if count == u16::MAX || size == u32::MAX || off == u32::MAX {
        return Err(Error::ArchiveInvalid);
    }
    if (off as usize)
        .checked_add(size as usize)
        .is_none_or(|n| n > data.len())
    {
        return Err(Error::ArchiveInvalid);
    }
    let mut zip = ZipArchive::new(Cursor::new(data)).map_err(ziperr)?;
    if zip.is_empty() {
        return Err(Error::ArchiveInvalid);
    }
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(ziperr)?;
        path_valid(f.name())?;
        if f.encrypted() {
            return Err(Error::ArchiveInvalid);
        }
    }
    Ok(zip)
}
fn import_to(
    saves: &fs::Dir,
    file: File,
    stem: &str,
    name: Option<&str>,
    replace: bool,
) -> Result<String> {
    let mut zip = archive(file)?;
    let mut prefix: Option<String> = None;
    let mut all = true;
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(ziperr)?;
        let Some((top, _)) = f.name().split_once('/') else {
            all = false;
            break;
        };
        if let Some(p) = &prefix {
            if p != top {
                all = false;
                break;
            }
        } else {
            prefix = Some(top.to_owned())
        }
    }
    if !all {
        prefix = None
    }
    let target = name.or(prefix.as_deref()).unwrap_or(stem).to_owned();
    validate_name(&target)?;
    let temp = unique(".jmcl-copy-", &target)?;
    let staging = saves.new_child(&temp)?;
    let result = (|| {
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(ziperr)?;
            let full = f.name().to_owned();
            let rel = if let Some(p) = &prefix {
                full.strip_prefix(&format!("{p}/")).unwrap_or(&full)
            } else {
                &full
            };
            if rel.is_empty() {
                continue;
            }
            if rel.ends_with('/') {
                staging.mkdir_relative(rel)?;
                continue;
            }
            if rel == "session.lock" {
                continue;
            }
            if f.size() > MAX_ARCHIVE as u64 {
                return Err(Error::ArchiveInvalid);
            }
            let mut data = vec![];
            (&mut f)
                .take(MAX_ARCHIVE as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|_| Error::ArchiveInvalid)?;
            if data.len() > MAX_ARCHIVE || data.len() as u64 != f.size() {
                return Err(Error::ArchiveInvalid);
            }
            staging.write_relative(rel, &data)?;
        }
        Ok(())
    })();
    // Windows validated directory handles intentionally deny share-delete.
    // Release the private staging handle before publishing its directory entry.
    drop(staging);
    let result = result.and_then(|()| {
        publish_staging(saves, &temp, &target, replace, &mut |old, new| {
            saves.rename(old, saves, new)
        })?;
        Ok(target)
    });
    let _ = saves.remove_tree(&temp);
    result
}
// Keep the original world recoverable if publication fails. The injected rename
// operation allows deterministic rollback testing without real game data/races.
pub(super) fn publish_staging(
    saves: &fs::Dir,
    temp: &str,
    target: &str,
    replace: bool,
    rename: &mut impl FnMut(&str, &str) -> Result<()>,
) -> Result<()> {
    let trash = if saves.exists(target) {
        if !replace {
            return Err(Error::WorldExists);
        }
        drop(guard(saves, target)?);
        let t = unique(".jmcl-trash-", target)?;
        rename(target, &t)?;
        Some(t)
    } else {
        None
    };
    if let Err(e) = rename(temp, target) {
        if let Some(t) = &trash {
            let _ = rename(t, target);
        }
        return Err(e);
    }
    if let Some(t) = trash {
        let _ = saves.remove_tree(t);
    }
    Ok(())
}
pub fn import(
    path: &str,
    file: &str,
    name: Option<&str>,
    replace: bool,
    version: Option<&str>,
) -> Result<Value> {
    let s = saves(path)?;
    let source = File::open(file).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Error::BackupNotFound
        } else {
            e.into()
        }
    })?;
    let stem = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or(Error::InvalidName)?;
    let name = import_to(&s, source, stem, name, replace)?;
    Ok(json!({"stage":"worlds.import","world":world(&s,&name,version)?}))
}
pub fn backup(path: &str, name: &str, label: Option<&str>) -> Result<Value> {
    let s = saves(path)?;
    let root = fs::Dir::root(Path::new(path))?;
    let b = root.child("backups", true)?;
    let now = time::OffsetDateTime::now_utc();
    let suffix = sanitize_label(label.unwrap_or(""));
    let mut filename = format!(
        "{name}-{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    if let Some(label) = suffix {
        filename.push('-');
        filename.push_str(&label)
    }
    filename.push_str(".zip");
    if b.exists(&filename) {
        return Err(Error::WorldExists);
    }
    export_to(&s, name, &b, &filename)?;
    Ok(json!({"stage":"worlds.backup","backup":filename}))
}
pub fn restore(
    path: &str,
    backup: &str,
    name: Option<&str>,
    replace: bool,
    version: Option<&str>,
) -> Result<Value> {
    validate_backup(backup)?;
    let s = saves(path)?;
    let root = fs::Dir::root(Path::new(path))?;
    let b = root.child("backups", false)?;
    let file = b.file(backup, false).map_err(|e| match e {
        Error::Io(e) if e.kind() == io::ErrorKind::NotFound => Error::BackupNotFound,
        _ => e,
    })?;
    let stem = backup.strip_suffix(".zip").unwrap_or(backup);
    let name = import_to(&s, file, stem, name, replace)?;
    Ok(json!({"stage":"worlds.restore","world":world(&s,&name,version)?}))
}

/// Archive a coupled server world set without reopening an ambient pathname.
#[cfg(target_os = "linux")]
pub(super) fn export_set(
    parent: &fs::Dir,
    names: &[String],
    destination: &fs::Dir,
    leaf: &str,
) -> Result<()> {
    for name in names {
        drop(guard(parent, name)?);
    }
    let tmp = unique(".jmcl-export-", leaf)?;
    let result = (|| {
        let file = destination.new_file(&tmp)?;
        let mut zip = ZipWriter::new(file);
        let mut files = 0;
        for name in names {
            let d = parent.child(name, false)?;
            add_dir(&mut zip, &d, name, &mut files)?;
        }
        let file = zip.finish().map_err(ziperr)?;
        file.sync_all()?;
        if file.metadata()?.len() > u32::MAX as u64 {
            return Err(Error::ArchiveTooLarge);
        }
        drop(file);
        destination.rename(&tmp, destination, leaf)
    })();
    if result.is_err() {
        let _ = destination.unlink(&tmp);
    }
    result
}
/// Preserve Zig's per-component publication/rollback semantics, not a claimed
/// all-world transaction. The outer server stopped guard spans every component.
#[cfg(target_os = "linux")]
pub(super) fn import_set(parent: &fs::Dir, file: File, replace: bool) -> Result<()> {
    let mut zip = archive(file)?;
    let mut components = Vec::<String>::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(ziperr)?;
        let (top, _) = entry.name().split_once('/').ok_or(Error::ArchiveInvalid)?;
        validate_name(top).map_err(|_| Error::ArchiveInvalid)?;
        if !components.iter().any(|s| s == top) {
            components.push(top.into());
        }
    }
    let temp = unique(".jmcl-copy-", "import")?;
    let staging = parent.new_child(&temp)?;
    let result = (|| {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(ziperr)?;
            let name = entry.name().to_owned();
            if name.ends_with('/') {
                staging.mkdir_relative(&name)?;
                continue;
            }
            if entry.size() > MAX_ARCHIVE as u64 {
                return Err(Error::ArchiveInvalid);
            }
            let mut bytes = Vec::new();
            (&mut entry)
                .take(MAX_ARCHIVE as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::ArchiveInvalid)?;
            if bytes.len() > MAX_ARCHIVE || bytes.len() as u64 != entry.size() {
                return Err(Error::ArchiveInvalid);
            }
            staging.write_relative(&name, &bytes)?;
        }
        for component in components {
            let trash = if parent.exists(&component) {
                if !replace {
                    return Err(Error::WorldExists);
                }
                let trash = unique(".jmcl-trash-", &component)?;
                parent.rename(&component, parent, &trash)?;
                Some(trash)
            } else {
                None
            };
            if let Err(e) = staging.rename(&component, parent, &component) {
                if let Some(trash) = trash {
                    let _ = parent.rename(&trash, parent, &component);
                }
                return Err(e);
            }
            if let Some(trash) = trash {
                let _ = parent.remove_tree(trash);
            }
        }
        Ok(())
    })();
    drop(staging);
    let _ = parent.remove_tree(temp);
    result
}

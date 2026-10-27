//! Native preparation and legacy assets: anchored, immutable inputs; private
//! extracted native directories are never imported back into the CAS.
use super::{Error, native_zip};
use crate::{
    plan::{File as PlannedFile, Plan, validate_relative_path},
    store::ArtifactRoot,
};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preparation {
    pub native_directory: Option<String>,
    pub native_files: usize,
    pub projections_created: usize,
    pub projections_cached: usize,
}
fn verified(root: &ArtifactRoot, expected: &PlannedFile) -> Result<File, Error> {
    verified_parts(root, &expected.path, &expected.sha1, expected.size)
}
fn verified_parts(root: &ArtifactRoot, path: &str, sha1: &str, size: u64) -> Result<File, Error> {
    let mut file = root.open_file(path)?;
    if file.metadata()?.len() != size {
        return Err(Error::named("CacheFileInvalid"));
    }
    let mut digest = Sha1::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    if !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(sha1) {
        return Err(Error::named("CacheFileInvalid"));
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}
/// Remove a native directory using only anchored no-follow descendant operations.
/// The relative name is constrained to one native session, not an arbitrary tree.
pub fn cleanup_native(root: &ArtifactRoot, relative: &str) -> Result<(), Error> {
    let suffix = relative
        .strip_prefix(".jmcl/runtime/natives/")
        .ok_or_else(|| Error::named("UnsafeRelativePath"))?;
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::named("UnsafeRelativePath"));
    }
    match root.remove_tree(relative) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn prepare(root: &ArtifactRoot, plan: &Plan) -> Result<Preparation, Error> {
    let mut result = Preparation {
        native_directory: None,
        native_files: 0,
        projections_created: 0,
        projections_cached: 0,
    };
    project_only(
        root,
        &plan.asset_projections,
        |path| {
            plan.files
                .iter()
                .find(|f| f.path == path)
                .map(|f| (f.sha1.as_str(), f.size))
        },
        &mut result,
    )?;
    let mut archives = Vec::new();
    let mut names = HashSet::new();
    let mut total = 0u64;
    for extraction in &plan.native_extractions {
        let expected = plan
            .files
            .iter()
            .find(|f| f.path == extraction.archive_path)
            .ok_or_else(|| Error::named("NativeArchiveNotPlanned"))?;
        let mut file = verified(root, expected)?;
        let entries = native_zip::entries(&mut file, |entry| {
            if extraction
                .exclude
                .iter()
                .any(|prefix| entry.name.starts_with(prefix.as_bytes()))
            {
                return Ok(false);
            }
            if ![0, 8].contains(&entry.method) {
                return Err(Error::named("UnsupportedCompressionMethod"));
            }
            if entry.size > 256 * 1024 * 1024 {
                return Err(Error::named("ArchiveEntryTooLarge"));
            }
            if (entry.size > 0 && entry.compressed == 0)
                || (entry.compressed > 0 && entry.size / entry.compressed > 1000)
            {
                return Err(Error::named("InvalidCompressionRatio"));
            }
            total = total
                .checked_add(entry.size)
                .ok_or_else(|| Error::named("NativeSizeOverflow"))?;
            if total > 1024 * 1024 * 1024 {
                return Err(Error::named("NativeArchiveSetTooLarge"));
            }
            if !entry.directory {
                result.native_files += 1;
                if !names.insert(entry.name.clone()) {
                    return Err(Error::named("ConflictingNativeEntry"));
                }
            }
            Ok(true)
        })?;
        archives.push((file, entries));
    }
    let (relative, native) = create_native_directory(root, secure_nonce)?;
    let extraction = (|| {
        for (file, entries) in &mut archives {
            for entry in entries {
                if entry.directory {
                    native_zip::extract(file, entry, &mut std::io::sink())?;
                    native.ensure_native_directory(&entry.name[..entry.name.len() - 1])?;
                } else {
                    let mut output = native.create_native_file(&entry.name)?;
                    native_zip::extract(file, entry, &mut output)?;
                    output.flush()?;
                }
            }
        }
        Ok::<(), Error>(())
    })();
    drop(native);
    if let Err(e) = extraction {
        let _ = cleanup_native(root, &relative);
        return Err(e);
    }
    result.native_directory = Some(relative);
    Ok(result)
}

/// Shared projection phase only: no native preparation or runtime callbacks.
pub(crate) fn project_only<'a>(
    root: &ArtifactRoot,
    projections: &[crate::plan::AssetProjection],
    lookup: impl Fn(&str) -> Option<(&'a str, u64)>,
    result: &mut Preparation,
) -> Result<(), Error> {
    for projection in projections {
        for path in [&projection.source_path, &projection.destination_path] {
            validate_relative_path(path).map_err(|e| Error::named(e.name()))?;
        }
        let (sha1, size) = lookup(&projection.source_path)
            .ok_or_else(|| Error::named("ProjectionSourceNotPlanned"))?;
        verified_parts(root, &projection.source_path, sha1, size)?;
        if root.verify(&projection.destination_path, sha1, size)? {
            result.projections_cached += 1;
            continue;
        }
        root.project(
            &projection.source_path,
            &projection.destination_path,
            sha1,
            size,
        )?;
        result.projections_created += 1;
    }
    Ok(())
}

pub(crate) fn secure_nonce(bytes: &mut [u8; 8]) -> Result<(), Error> {
    getrandom::fill(bytes).map_err(|_| Error::named("RandomSourceFailed"))
}
/// Same bounded exclusive creation loop as the String preparer.
pub(crate) fn create_native_directory(
    root: &ArtifactRoot,
    mut nonce: impl FnMut(&mut [u8; 8]) -> Result<(), Error>,
) -> Result<(String, ArtifactRoot), Error> {
    root.ensure_directory(".jmcl/runtime/natives")?;
    let mut chosen = None;
    for _ in 0..16 {
        let mut bytes = [0; 8];
        nonce(&mut bytes)?;
        let relative = format!(".jmcl/runtime/natives/{:x}", u64::from_ne_bytes(bytes));
        match root.create_directory(&relative) {
            Ok(dir) => {
                chosen = Some((relative, dir));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    chosen.ok_or_else(|| Error::named("NativeDirectoryCollision"))
}

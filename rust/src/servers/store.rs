//! Server metadata and installation records; paths remain compatible with Zig.
use super::{
    lifecycle::{atomic_write, open, read},
    manifest::Manifest,
};
use rustix::fs::{self, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::{fs::File, io};
fn invalid(e: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}
pub fn open_root(path: &str) -> io::Result<File> {
    open_root_native(path.as_ref())
}
pub(crate) fn open_root_native(path: &std::ffi::OsStr) -> io::Result<File> {
    if path.is_empty() {
        return Err(invalid("EmptyServersRoot"));
    }
    // Let the unchanged nofollow directory open distinguish an existing file
    // from a directory, as the source's createDirPathOpen does.
    match std::fs::create_dir_all(std::path::Path::new(path)) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(Into::into)
}
pub fn open_server(root: &File, id: &str) -> io::Result<File> {
    crate::instances::validate_id(id).map_err(|_| invalid("InvalidServerId"))?;
    open(
        root,
        id,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
        0,
    )
}
pub fn load(root: &File, id: &str) -> io::Result<Manifest> {
    let dir = open_server(root, id)?;
    Manifest::parse(&read(&dir, "server.json", 64 * 1024)?, id).map_err(invalid)
}
pub(crate) fn load_read(root: &File, id: &str) -> io::Result<super::manifest::ReadManifest> {
    let dir = open_server(root, id)?;
    super::manifest::ReadManifest::parse(&read(&dir, "server.json", 64 * 1024)?, id)
        .map_err(invalid)
}
pub fn write_manifest(dir: &File, manifest: &Manifest) -> io::Result<()> {
    atomic_write(dir, "server.json", &manifest.encode().map_err(invalid)?)
}
fn schema() -> u32 {
    3
}
fn official() -> String {
    "official".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    Jar,
    ArgsFile,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    #[serde(
        default = "schema",
        deserialize_with = "crate::instances::schema_number"
    )]
    pub schema_version: u32,
    pub version_id: String,
    #[serde(default)]
    pub fabric_loader: Option<String>,
    #[serde(default)]
    pub neoforge_version: Option<String>,
    #[serde(default)]
    pub forge_version: Option<String>,
    #[serde(default = "official")]
    pub source: String,
    pub server_sha1: String,
    #[serde(deserialize_with = "super::numbers::persisted_u64")]
    pub server_size: u64,
    #[serde(deserialize_with = "super::numbers::persisted_u16")]
    pub java_major_version: u16,
    pub launch_mode: LaunchMode,
    pub launch_file: String,
    pub launch_sha1: String,
    #[serde(deserialize_with = "super::numbers::persisted_u64")]
    pub launch_size: u64,
}
impl Installation {
    pub fn valid(&self) -> bool {
        self.schema_version == 3
            && !self.version_id.is_empty()
            && self.version_id.len() <= 256
            && [
                self.fabric_loader.is_some(),
                self.neoforge_version.is_some(),
                self.forge_version.is_some(),
            ]
            .into_iter()
            .filter(|b| *b)
            .count()
                <= 1
            && matches!(self.source.as_str(), "official" | "bmclapi")
            && self.server_size > 0
            && self.java_major_version > 0
            && self.launch_size > 0
            && self.server_sha1.len() == 40
            && self.server_sha1.bytes().all(|b| b.is_ascii_hexdigit())
            && self.launch_sha1.len() == 40
            && self
                .launch_sha1
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && valid_relative(&self.launch_file)
    }
}
fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|p| {
            if p.is_empty()
                || matches!(p, "." | "..")
                || p.ends_with(['.', ' '])
                || p.bytes()
                    .any(|b| b < 32 || b == 127 || matches!(b, b'\\' | b':'))
            {
                return false;
            }
            let stem = p.split('.').next().unwrap().to_ascii_uppercase();
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && (b'1'..=b'9').contains(&stem.as_bytes()[3]))
        })
}
pub fn load_installation(dir: &File) -> io::Result<Option<Installation>> {
    let bytes = match read(dir, ".jmcl/install.json", 16 * 1024) {
        Ok(b) => b,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::InvalidData
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    Ok(serde_json::from_slice::<Installation>(&bytes)
        .ok()
        .filter(Installation::valid))
}
pub fn installed_java_major(dir: &File, m: &Manifest) -> io::Result<Option<u16>> {
    installed_identity(dir, m.identity(), &m.source)
}
pub(crate) fn installed_identity(
    dir: &File,
    m: crate::instances::InstallationIdentity<'_>,
    source: &str,
) -> io::Result<Option<u16>> {
    let Some(i) = load_installation(dir)? else {
        return Ok(None);
    };
    if i.version_id != m.version_id
        || &i.fabric_loader != m.fabric_loader
        || &i.neoforge_version != m.neoforge_version
        || &i.forge_version != m.forge_version
        || i.source != source
    {
        return Ok(None);
    }
    for (path, size) in [
        ("server.jar", i.server_size),
        (i.launch_file.as_str(), i.launch_size),
    ] {
        let stat = match fs::statat(dir, path, fs::AtFlags::empty()) {
            Ok(s) => s,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if fs::FileType::from_raw_mode(stat.st_mode) != fs::FileType::RegularFile
            || stat.st_size < 0
            || stat.st_size as u64 != size
        {
            return Ok(None);
        }
    }
    Ok(Some(i.java_major_version))
}

#[cfg(test)]
mod root_tests {
    use super::*;
    #[test]
    fn root_open_preserves_directory_errors_and_nofollow() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("file");
        std::fs::write(&file, b"owned").unwrap();
        for path in [&file, &file.join("child")] {
            assert_eq!(
                open_root_native(path.as_os_str())
                    .unwrap_err()
                    .raw_os_error(),
                Some(libc::ENOTDIR)
            );
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"owned");
        let directory = t.path().join("created/nested");
        let handle = open_root_native(directory.as_os_str()).unwrap();
        assert!(handle.metadata().unwrap().is_dir());
        assert!(
            open_root_native(directory.as_os_str())
                .unwrap()
                .metadata()
                .unwrap()
                .is_dir()
        );
        let alias = t.path().join("alias");
        std::os::unix::fs::symlink(&directory, &alias).unwrap();
        assert_eq!(
            open_root_native(alias.as_os_str())
                .unwrap_err()
                .raw_os_error(),
            Some(libc::ENOTDIR)
        );
        assert!(alias.is_symlink());
        assert_eq!(open_root("").unwrap_err().to_string(), "EmptyServersRoot");
    }
}

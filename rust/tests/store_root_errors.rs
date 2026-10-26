//! Direct owned root APIs only. No installer/archive/provider callers.
use jmcl_core_rust::store::{ArtifactRoot, Store};
use std::{fs, io, path::Path};
fn open_store(path: &Path) -> io::Result<()> {
    Store::open(path).map(drop)
}
fn open_artifact(path: &Path) -> io::Result<()> {
    ArtifactRoot::open(path).map(drop)
}
#[cfg(unix)]
fn identity(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path).unwrap();
    (m.dev(), m.ino())
}
#[cfg(windows)]
fn identity(path: &Path) -> (u64, u64) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let file = fs::File::open(path).unwrap();
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    assert_ne!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) },
        0
    );
    (
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    )
}
#[test]
fn existing_regular_root_reports_not_directory_without_mutation() {
    for open in [open_store as fn(&Path) -> io::Result<()>, open_artifact] {
        let t = tempfile::tempdir().unwrap();
        let blocked = t.path().join("blocked");
        fs::write(&blocked, b"owned").unwrap();
        let peer = t.path().join("peer");
        fs::hard_link(&blocked, &peer).unwrap();
        let before = fs::metadata(&blocked).unwrap();
        let before_identity = identity(&blocked);
        assert_eq!(identity(&peer), before_identity);
        assert_eq!(
            open(&blocked).unwrap_err().kind(),
            io::ErrorKind::NotADirectory
        );
        let after = fs::metadata(&blocked).unwrap();
        assert_eq!(identity(&blocked), before_identity);
        assert_eq!(identity(&peer), before_identity);
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        assert_eq!(before.permissions(), after.permissions());
        assert_eq!(fs::read(&blocked).unwrap(), b"owned");
        assert_eq!(fs::read(&peer).unwrap(), b"owned");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                (before.dev(), before.ino(), before.nlink()),
                (after.dev(), after.ino(), after.nlink())
            );
        }
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 2);
        assert_eq!(
            open(&blocked.join("child")).unwrap_err().kind(),
            io::ErrorKind::NotADirectory
        );
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 2);
    }
}
#[test]
fn existing_and_missing_owned_root_creation_stays_exact() {
    let t = tempfile::tempdir().unwrap();
    let cas = t.path().join("parent/cas");
    open_store(&cas).unwrap();
    open_store(&cas).unwrap();
    for path in ["objects/sha1", "locks/sha1"] {
        assert!(cas.join(path).is_dir());
    }
    assert_eq!(fs::read_dir(&cas).unwrap().count(), 2);
    let artifact = t.path().join("artifact");
    open_artifact(&artifact).unwrap();
    open_artifact(&artifact).unwrap();
    assert_eq!(fs::read_dir(&artifact).unwrap().count(), 0);
}
#[cfg(unix)]
fn directory_alias(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}
#[cfg(windows)]
fn directory_alias(target: &Path, link: &Path) {
    let o = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "owned junction creation: {:?} {:?}",
        o.stdout,
        o.stderr
    );
}
#[cfg(any(unix, windows))]
#[test]
fn final_alias_rejected_while_owned_ancestor_alias_policy_is_preserved() {
    for open in [open_store as fn(&Path) -> io::Result<()>, open_artifact] {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("real");
        fs::create_dir(&real).unwrap();
        let marker = real.join("marker");
        fs::write(&marker, b"preserve").unwrap();
        let alias = t.path().join("alias");
        directory_alias(&real, &alias);
        assert!(open(&alias).is_err());
        assert_eq!(fs::read_dir(&real).unwrap().count(), 1);
        assert_eq!(fs::read(&marker).unwrap(), b"preserve");
        #[cfg(unix)]
        {
            let file_alias = t.path().join("file-alias");
            std::os::unix::fs::symlink(&marker, &file_alias).unwrap();
            assert!(open(&file_alias).is_err());
            assert_eq!(fs::read(&marker).unwrap(), b"preserve");
        }
        open(&alias.join("child")).unwrap();
        assert!(real.join("child").is_dir());
        assert_eq!(fs::read(&marker).unwrap(), b"preserve");
    }
}

#[test]
fn malformed_owned_root_preserves_non_already_exists_error() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("bad\0path");
    let original = fs::create_dir_all(&path).unwrap_err();
    assert_ne!(original.kind(), io::ErrorKind::AlreadyExists);
    for open in [open_store as fn(&Path) -> io::Result<()>, open_artifact] {
        let error = open(&path).unwrap_err();
        assert_eq!(error.kind(), original.kind());
        assert_eq!(error.raw_os_error(), original.raw_os_error());
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 0);
    }
}

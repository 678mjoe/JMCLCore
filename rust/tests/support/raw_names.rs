//! Probe the temporary fixture filesystem, not the operating-system name.
//! APFS may reject non-UTF8 names with EILSEQ. Only that observed capability
//! rejection is accepted; permissions, exhausted space, etc. remain test failures.
use std::{fs, io, os::unix::ffi::OsStrExt, path::Path};

#[allow(
    dead_code,
    reason = "each integration-test binary uses only its relevant probe kind"
)]
pub enum Kind {
    File,
    Directory,
}
pub enum Support {
    Supported,
    Rejected(io::Error),
}
pub fn probe(parent: &Path, kind: Kind) -> Support {
    let path = parent.join(std::ffi::OsStr::from_bytes(b".raw-name-probe-\xff"));
    let result = match kind {
        Kind::File => fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(drop),
        Kind::Directory => fs::create_dir(&path),
    };
    match result {
        Ok(()) => {
            match kind {
                Kind::File => fs::remove_file(&path),
                Kind::Directory => fs::remove_dir(&path),
            }
            .expect("remove only the successfully created raw-name probe");
            Support::Supported
        }
        Err(error) if error.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error()) => {
            Support::Rejected(error)
        }
        Err(error) => panic!("unexpected raw-name capability probe failure: {error}"),
    }
}

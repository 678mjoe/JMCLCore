//! macOS libc spawnp may interpret an ENOEXEC file through /bin/sh. The Zig
//! oracle calls execve directly, so use a preallocated execve callback instead.
//! All allocation happens before fork; the child only calls execve/reads errno.
use super::detect::Environment;
use std::ffi::{c_char, c_int};
use std::{
    ffi::CString,
    io,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    process::Command,
};
unsafe extern "C" {
    fn execve(path: *const c_char, argv: *const *const c_char, envp: *const *const c_char)
    -> c_int;
}
struct Image {
    paths: Vec<CString>,
    _args: Vec<CString>,
    _env: Vec<CString>,
    args: Vec<*const c_char>,
    env: Vec<*const c_char>,
}
// The pointers reference immutable CString allocations owned by this Image.
// Moving Image never moves those allocations; no pointer escapes its lifetime.
unsafe impl Send for Image {}
unsafe impl Sync for Image {}
impl Image {
    fn execute(&self) -> io::Result<()> {
        let mut denied = false;
        let mut last = 2;
        for path in &self.paths {
            // SAFETY: all arrays are null-terminated and all strings are owned
            // by self. Successful execve never returns. No shell is invoked.
            unsafe {
                execve(path.as_ptr(), self.args.as_ptr(), self.env.as_ptr());
            }
            let error = io::Error::last_os_error();
            last = error.raw_os_error().unwrap_or(22);
            match last {
                13 => denied = true,
                2 | 20 => {}
                _ => return Err(error),
            }
        }
        Err(io::Error::from_raw_os_error(if denied { 13 } else { last }))
    }
}
fn cstring(bytes: impl Into<Vec<u8>>) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}
pub(super) fn configure(
    cmd: &mut Command,
    executable: &str,
    args: &[&str],
    env: &Environment,
) -> io::Result<()> {
    let paths = if executable.contains('/') {
        vec![cstring(executable)?]
    } else {
        // Zig Threaded.posixExecv uses the parent PATH, skips empty components,
        // and only falls back to this default when PATH is absent.
        let path =
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/bin/:/usr/bin".into());
        path.as_bytes()
            .split(|b| *b == b':')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let mut b = p.to_vec();
                b.push(b'/');
                b.extend_from_slice(executable.as_bytes());
                cstring(b)
            })
            .collect::<io::Result<Vec<_>>>()?
    };
    let owned_args = std::iter::once(executable)
        .chain(args.iter().copied())
        .map(cstring)
        .collect::<io::Result<Vec<_>>>()?;
    let owned_env = env
        .iter()
        .map(|(k, v)| {
            let mut b = k.as_bytes().to_vec();
            b.push(b'=');
            b.extend_from_slice(v.as_bytes());
            cstring(b)
        })
        .collect::<io::Result<Vec<_>>>()?;
    let pointers = |strings: &Vec<CString>| {
        strings
            .iter()
            .map(|s| s.as_ptr())
            .chain(std::iter::once(std::ptr::null()))
            .collect()
    };
    let image = Image {
        paths,
        args: pointers(&owned_args),
        env: pointers(&owned_env),
        _args: owned_args,
        _env: owned_env,
    };
    // SAFETY: callback performs no allocation, locking, formatting or unwinding.
    // Returning the execve errno uses std's existing child-error pipe/reaping.
    unsafe {
        cmd.pre_exec(move || image.execute());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn direct_exec_preserves_invalid_format_and_legitimate_shebang() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("probe");
        let env = Environment::new();
        std::fs::write(&file, b"not-an-executable").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut cmd = Command::new(&file);
        configure(&mut cmd, file.to_str().unwrap(), &[], &env).unwrap();
        assert_eq!(cmd.output().unwrap_err().raw_os_error(), Some(8));
        std::fs::write(&file, b"#!/bin/sh\nprintf 'literal output'\nexit 7\n").unwrap();
        let mut cmd = Command::new(&file);
        configure(&mut cmd, file.to_str().unwrap(), &[], &env).unwrap();
        let result = cmd.output().unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert_eq!(result.stdout, b"literal output");
    }
}

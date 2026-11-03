//! Pure top-level native argv diagnostics, before String conversion.
use std::{
    ffi::OsString,
    io::{self, Write},
};
pub fn diagnostics(
    args: &[OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<io::Result<u8>> {
    if !args.iter().any(|v| v.to_str().is_none()) {
        return None;
    }
    let command = args.first()?;
    match command.to_str() {
        Some("help" | "--help") => Some(out.write_all(include_bytes!("help.txt")).map(|_| 0)),
        Some("rpc") => Some(err.write_all(b"usage: jmcl-core rpc\n").map(|_| 2)),
        Some("version")
            if !matches!(args.get(1).and_then(|v| v.to_str()), Some("list" | "show")) =>
        {
            Some(
                err.write_all(b"usage: jmcl-core version [--json]\n")
                    .map(|_| 2),
            )
        }
        Some(
            "version"
            | "install"
            | "store"
            | "java"
            | "instance"
            | "server"
            | "launch"
            | "__server_bootstrap"
            | "__server_supervisor",
        ) => None,
        _ => Some((|| {
            err.write_all(b"unknown command: ")?;
            err.write_all(&crate::native_bytes::os_bytes(command))?;
            err.write_all(b"\nrun `jmcl-core --help` for usage\n")?;
            Ok(2)
        })()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![255])
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xd800])
        }
    }
    #[test]
    fn unknown_command_bytes_are_not_replaced() {
        let raw = raw();
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            diagnostics(std::slice::from_ref(&raw), &mut out, &mut err)
                .unwrap()
                .unwrap(),
            2
        );
        let mut expected = b"unknown command: ".to_vec();
        expected.extend(crate::native_bytes::os_bytes(&raw));
        expected.extend(b"\nrun `jmcl-core --help` for usage\n");
        assert_eq!(err, expected);
        assert!(out.is_empty());
    }
    #[test]
    fn help_and_usage_are_pure_even_with_raw_tail() {
        for (command, code, stdout, stderr) in [
            (
                "help",
                0,
                include_bytes!("help.txt").as_slice(),
                b"".as_slice(),
            ),
            (
                "--help",
                0,
                include_bytes!("help.txt").as_slice(),
                b"".as_slice(),
            ),
            (
                "rpc",
                2,
                b"".as_slice(),
                b"usage: jmcl-core rpc\n".as_slice(),
            ),
            (
                "version",
                2,
                b"".as_slice(),
                b"usage: jmcl-core version [--json]\n".as_slice(),
            ),
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                diagnostics(&[command.into(), raw()], &mut out, &mut err)
                    .unwrap()
                    .unwrap(),
                code
            );
            assert_eq!(out, stdout);
            assert_eq!(err, stderr);
        }
    }
    #[test]
    fn known_operational_routes_and_utf8_are_not_claimed() {
        for command in [
            "install",
            "store",
            "java",
            "instance",
            "server",
            "launch",
            "__server_bootstrap",
            "__server_supervisor",
        ] {
            assert!(
                diagnostics(&[command.into(), raw()], &mut Vec::new(), &mut Vec::new()).is_none()
            );
        }
        for method in ["list", "show"] {
            assert!(
                diagnostics(
                    &["version".into(), method.into(), raw()],
                    &mut Vec::new(),
                    &mut Vec::new()
                )
                .is_none()
            );
        }
        assert!(diagnostics(&["--help".into()], &mut Vec::new(), &mut Vec::new()).is_none());
    }
}

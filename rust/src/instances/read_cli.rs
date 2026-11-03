//! Native argv bridge for instance list/show. Existing root-open semantics apply.
use super::{Root, byte_read, cli_error_name, validate_id};
use serde::Serialize;
use std::{
    ffi::OsString,
    io::{self, Write},
};
struct Options {
    directory: OsString,
    id: Option<String>,
    json: bool,
}
fn parse(command: &str, args: &[OsString]) -> Option<Options> {
    let mut o = Options {
        directory: "instances".into(),
        id: None,
        json: false,
    };
    let mut i = 0;
    if command == "show" {
        let id = args.first()?.to_str()?;
        validate_id(id).ok()?;
        o.id = Some(id.into());
        i = 1;
    }
    while i < args.len() {
        match args[i].to_str() {
            Some("--json") => {
                o.json = true;
                i += 1;
            }
            Some("--instances-directory") => {
                o.directory = args.get(i + 1).filter(|v| !v.is_empty())?.clone();
                i += 2;
            }
            _ => return None,
        }
    }
    Some(o)
}
pub fn run(
    args: &[OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<io::Result<u8>> {
    if args.first().and_then(|v| v.to_str()) != Some("instance")
        || !args.iter().any(|v| v.to_str().is_none())
    {
        return None;
    }
    let command = args.get(1)?.to_str()?;
    if !matches!(command, "list" | "show") {
        return None;
    }
    Some(inner(command, &args[2..], out, err))
}
fn inner(
    command: &str,
    args: &[OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let Some(o) = parse(command, args) else {
        writeln!(
            err,
            "usage: jmcl-core instance {command}{} [--instances-directory PATH] [--json]",
            if command == "show" { " ID" } else { "" }
        )?;
        return Ok(2);
    };
    let root = match Root::open_native(&o.directory) {
        Ok(r) => r,
        Err(e) => {
            writeln!(
                err,
                "failed to open instances directory: {}",
                cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    if let Some(id) = o.id {
        let m = match root.load_bytes(&id) {
            Ok(m) => m,
            Err(e) => {
                writeln!(err, "failed to load instance: {}", cli_error_name(&e))?;
                return Ok(1);
            }
        };
        let bytes = super::DisplayName(display_path(
            crate::native_bytes::os_bytes(&o.directory),
            &id,
            cfg!(windows),
        ));
        if o.json {
            #[derive(Serialize)]
            struct Show<'a> {
                instance: &'a byte_read::Manifest,
                directory: &'a super::DisplayName,
            }
            serde_json::to_writer(
                &mut *out,
                &Show {
                    instance: &m,
                    directory: &bytes,
                },
            )?;
            out.write_all(b"\n")?;
        } else {
            write!(out, "instance\t{}\nname\t", m.id())?;
            out.write_all(&m.name.0)?;
            out.write_all(b"\nversion\t")?;
            out.write_all(&m.version_id.0)?;
            write!(out, "\nsource\t{}\ndirectory\t", m.source)?;
            out.write_all(&bytes.0)?;
            out.write_all(b"\n")?;
        }
    } else {
        let manifests = match root.list_bytes() {
            Ok(m) => m,
            Err(e) => {
                writeln!(err, "failed to list instances: {}", cli_error_name(&e))?;
                return Ok(1);
            }
        };
        if o.json {
            #[derive(Serialize)]
            struct List<'a> {
                instances: &'a [byte_read::Manifest],
            }
            serde_json::to_writer(
                &mut *out,
                &List {
                    instances: &manifests,
                },
            )?;
            out.write_all(b"\n")?;
        } else {
            for m in manifests {
                write!(out, "{}\t", m.id())?;
                out.write_all(&m.version_id.0)?;
                write!(out, "\t{}\t", m.source)?;
                out.write_all(&m.name.0)?;
                out.write_all(b"\n")?;
            }
        }
    }
    Ok(0)
}

// ID has already been validated: both appended components are nonempty and have
// no separators. Match Zig's naive join, including Windows drive-relative roots.
fn display_path(mut root: Vec<u8>, id: &str, windows: bool) -> Vec<u8> {
    let sep = if windows { b'\\' } else { b'/' };
    if !root
        .last()
        .is_some_and(|b| *b == b'/' || (windows && *b == b'\\'))
    {
        root.push(sep);
    }
    root.extend_from_slice(id.as_bytes());
    root.push(sep);
    root.extend_from_slice(b".minecraft");
    root
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
    fn raw_id_and_flags_fail_before_root_open() {
        for args in [
            vec![raw()],
            vec!["demo".into(), raw()],
            vec![
                "demo".into(),
                "--instances-directory".into(),
                raw(),
                "--force".into(),
            ],
        ] {
            assert!(parse("show", &args).is_none());
        }
        assert!(
            parse(
                "list",
                &[
                    "--instances-directory".into(),
                    raw(),
                    "--instances-directory".into(),
                    "".into()
                ]
            )
            .is_none()
        );
    }
    #[test]
    fn repeated_options_and_flag_shaped_path_are_preserved() {
        let o = parse(
            "list",
            &[
                "--instances-directory".into(),
                raw(),
                "--instances-directory".into(),
                "--json".into(),
                "--json".into(),
            ],
        )
        .unwrap();
        assert_eq!(o.directory, "--json");
        assert!(o.json);
        assert_eq!(
            parse(
                "show",
                &["demo".into(), "--instances-directory".into(), raw()]
            )
            .unwrap()
            .directory,
            raw()
        );
    }
    #[test]
    fn unrelated_commands_and_utf8_remain_unclaimed() {
        for args in [
            vec!["instance".into(), "create".into(), raw()],
            vec!["instance".into(), "delete".into(), raw()],
            vec!["instance".into(), "list".into()],
        ] {
            assert!(run(&args, &mut Vec::new(), &mut Vec::new()).is_none());
        }
    }
    #[test]
    fn display_join_preserves_native_spelling_without_normalization() {
        for (root, windows, expected) in [
            (
                b"raw-\xff//".as_slice(),
                false,
                b"raw-\xff//demo/.minecraft".as_slice(),
            ),
            (b"./raw-\xff/./", false, b"./raw-\xff/./demo/.minecraft"),
            (b"C:", true, b"C:\\demo\\.minecraft"),
            (
                b"C:/raw-\xed\xa0\x80/",
                true,
                b"C:/raw-\xed\xa0\x80/demo\\.minecraft",
            ),
            (b"C:\\raw\\\\", true, b"C:\\raw\\\\demo\\.minecraft"),
        ] {
            assert_eq!(display_path(root.to_vec(), "demo", windows), expected);
        }
    }
}

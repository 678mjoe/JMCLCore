use super::*;
use std::io::{self, Write};
/// Receives arguments after `instance`; None means this slice does not own it.
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Option<io::Result<u8>> {
    let kind = Kind::from_directory(args.first()?.as_str())?;
    let op = args.get(1).map(String::as_str).unwrap_or("");
    if op == "adopt" {
        return Some(adopt(kind, &args[2..], out, err));
    }
    if matches!(op, "install" | "set-version") {
        return Some(super::cli_remote::run_with(
            kind,
            op,
            &args[2..],
            out,
            err,
            &super::http::Native,
            &super::http::Native,
        ));
    }
    Some(inner(kind, op, &args[2.min(args.len())..], out, err))
}
fn usage(kind: Kind, op: &str) -> String {
    let family = kind.directory();
    let world = if kind == Kind::Datapack {
        " --world NAME"
    } else {
        ""
    };
    if op == "list" {
        format!(
            "usage: jmcl-core instance {family} list ID{world} [--instances-directory PATH] [--json]\n"
        )
    } else if matches!(op, "enable" | "disable" | "remove") {
        format!(
            "usage: jmcl-core instance {family} {op} ID{world} (--project REF | --file PATH) [--instances-directory PATH]\n"
        )
    } else {
        format!(
            "usage: jmcl-core instance {family} <list|install|set-version|enable|disable|remove|adopt> ID{world} [options]\n"
        )
    }
}
fn bad(err: &mut impl Write) -> io::Result<u8> {
    err.write_all(b"error: InvalidArguments\n")?;
    Ok(1)
}
pub(super) fn name(e: &Error) -> String {
    match e {
        Error::Io(e) => crate::instances::cli_error_name(e),
        Error::Invalid => "ContentSelectorRequired".into(),
        Error::Selector(name) => (*name).into(),
        Error::Registry(e) => (*e).into(),
        Error::NotFound => "ContentNotFound".into(),
        Error::WorldNotFound => "WorldNotFound".into(),
        Error::WorldLocked => "WorldLocked".into(),
        Error::WorldInvalid => "InvalidName".into(),
    }
}
fn inner(
    kind: Kind,
    op: &str,
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let args: Vec<_> = args.iter().map(|a| LocalArg::Text(a.as_str())).collect();
    local_inner(kind, op, &args, out, err)
}
pub(super) fn local_inner(
    kind: Kind,
    op: &str,
    args: &[LocalArg<'_>],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    if !matches!(op, "list" | "enable" | "disable" | "remove") || args.is_empty() {
        err.write_all(usage(kind, op).as_bytes())?;
        return Ok(2);
    }
    let Some(id) = args[0].text() else {
        return bad(err);
    };
    if crate::instances::validate_id(id).is_err() {
        return bad(err);
    }
    let datapack = kind == Kind::Datapack;
    let family = kind.directory();
    let mut directory = std::ffi::OsString::from("instances");
    let (mut world, mut project, mut file) = (None, None, None);
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        let Some(a) = args[i].text() else {
            return bad(err);
        };
        if a == "--json" && op == "list" {
            json = true;
            i += 1;
            continue;
        }
        let Some(v) = args.get(i + 1) else {
            return bad(err);
        };
        if v.bytes().is_empty() && !(op == "list" && !datapack && a == "--instances-directory") {
            return bad(err);
        }
        match a {
            "--instances-directory" => {
                directory = crate::store::path_from_bytes(v.bytes())?;
            }
            "--world" if datapack => world = Some(v.bytes()),
            "--project" if op != "list" => {
                project = Some(match v.text() {
                    Some(v) => v,
                    None => return bad(err),
                })
            }
            "--file" if op != "list" => file = Some(v.bytes()),
            _ => return bad(err),
        }
        i += 2;
    }
    if datapack && world.is_none() {
        writeln!(err, "instance datapacks {op} requires --world")?;
        return Ok(2);
    }
    if op != "list" && project.is_some() == file.is_some() {
        writeln!(
            err,
            "instance {family} {op} requires exactly one of --project or --file"
        )?;
        return Ok(2);
    }
    let game = std::path::Path::new(&directory).join(id).join(".minecraft");
    let root = if datapack {
        match Root::world_bytes_native(game.as_os_str(), world.unwrap(), op != "list") {
            Ok(r) => r,
            Err(e) => {
                world_error(err, &e, op)?;
                return Ok(1);
            }
        }
    } else {
        let instances = match crate::instances::Root::open_native(&directory) {
            Ok(r) => r,
            Err(e) => {
                writeln!(
                    err,
                    "failed to open instances directory: {}",
                    crate::instances::cli_error_name(&e)
                )?;
                return Ok(1);
            }
        };
        if let Err(e) = instances.load_read(id) {
            writeln!(
                err,
                "failed to load instance: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
        match Root::open_native(game.as_os_str()) {
            Ok(r) => r,
            Err(e) => {
                writeln!(err, "failed to open instance directory: {}", name(&e))?;
                return Ok(1);
            }
        }
    };
    if op == "list" {
        let (entries, unmanaged) = match root.list_paths(kind) {
            Ok(v) => v,
            Err(e) => {
                if datapack {
                    world_error(err, &e, op)?
                } else {
                    writeln!(err, "failed to list instance content: {}", name(&e))?
                }
                return Ok(1);
            }
        };
        if json {
            let mut m = Map::new();
            m.insert("entries".into(), val(entries));
            m.insert("unmanaged".into(), val(unmanaged));
            serde_json::to_writer(&mut *out, &Value::Object(m))?;
            out.write_all(b"\n")?;
        } else {
            for e in entries {
                let kind = serde_json::to_value(e.kind).unwrap();
                let provider = serde_json::to_value(e.source.provider).unwrap();
                write!(out, "{}\t", kind.as_str().unwrap())?;
                out.write_all(&e.file.0)?;
                writeln!(
                    out,
                    "\t{}\t{}\t{}\t{}\t{}\t{}",
                    e.sha1,
                    e.size,
                    provider.as_str().unwrap(),
                    e.source.slug.as_deref().unwrap_or("-"),
                    if e.enabled { "enabled" } else { "disabled" },
                    if e.as_dependency {
                        "dependency"
                    } else {
                        "explicit"
                    }
                )?;
            }
            for u in unmanaged {
                out.write_all(b"unmanaged\t")?;
                out.write_all(&u.0)?;
                out.write_all(b"\n")?;
            }
        }
        return Ok(0);
    }
    if op == "remove" {
        match root.remove_bytes(kind, project, file) {
            Ok(e) => {
                out.write_all(b"removed\t")?;
                out.write_all(&e.file.0)?;
                writeln!(out, "\t{}", e.sha1)?;
            }
            Err(e) => {
                mutation_error(err, &e, kind, op)?;
                return Ok(1);
            }
        }
    } else {
        match root.toggle_bytes(kind, project, file, op == "enable") {
            Ok((changed, e)) => {
                let state = if op == "enable" {
                    "enabled"
                } else {
                    "disabled"
                };
                if changed {
                    write!(out, "{state}\t")?;
                    out.write_all(&e.file.0)?;
                    writeln!(out)?
                } else {
                    out.write_all(b"unchanged\t")?;
                    out.write_all(&e.file.0)?;
                    writeln!(out, "\t{state}")?
                }
            }
            Err(e) => {
                mutation_error(err, &e, kind, op)?;
                return Ok(1);
            }
        }
    }
    Ok(0)
}
fn mutation_error(err: &mut impl Write, e: &Error, kind: Kind, op: &str) -> io::Result<()> {
    if kind == Kind::Datapack {
        return world_error(err, e, op);
    }
    if matches!(e, Error::NotFound) {
        writeln!(
            err,
            "no registered {} matches the given selector",
            serde_json::to_value(kind).unwrap().as_str().unwrap()
        )
    } else {
        writeln!(err, "failed to {op} content: {}", name(e))
    }
}
fn world_error(err: &mut impl Write, e: &Error, op: &str) -> io::Result<()> {
    match e {
        Error::Registry("InvalidCurseforgeId") if op == "list" => {
            err.write_all(b"curseforge projects and files use numeric ids\n")
        }
        Error::WorldNotFound => err.write_all(b"no world matches the given name\n"),
        Error::WorldLocked => err.write_all(b"the world is in use by a running game\n"),
        Error::NotFound => err.write_all(b"no registered datapack matches the given selector\n"),
        _ => writeln!(err, "datapack {op} failed: {}", name(e)),
    }
}

fn adopt(
    kind: Kind,
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let dp = kind == Kind::Datapack;
    if args.is_empty() {
        writeln!(
            err,
            "usage: jmcl-core instance {} adopt ID{} [--instances-directory PATH] [--store-directory PATH]",
            kind.directory(),
            if dp { " --world NAME" } else { "" }
        )?;
        return Ok(2);
    }
    let id = &args[0];
    if crate::instances::validate_id(id).is_err() {
        return bad(err);
    }
    let mut directory = "instances";
    let mut store_directory = crate::store::DEFAULT_DIRECTORY;
    let mut world = None;
    let mut i = 1;
    while i < args.len() {
        let Some(v) = args.get(i + 1) else {
            return bad(err);
        };
        if v.is_empty() {
            return bad(err);
        };
        match args[i].as_str() {
            "--instances-directory" => directory = v,
            "--store-directory" => store_directory = v,
            "--world" if dp => world = Some(v.as_str()),
            _ => return bad(err),
        }
        i += 2;
    }
    if dp && world.is_none() {
        err.write_all(b"instance datapacks adopt requires --world\n")?;
        return Ok(2);
    }
    let game = crate::instances::minecraft_path(directory, id);
    let mut content = None;
    if !dp {
        let instances = match crate::instances::Root::open(directory) {
            Ok(r) => r,
            Err(e) => {
                writeln!(
                    err,
                    "failed to open instances directory: {}",
                    crate::instances::cli_error_name(&e)
                )?;
                return Ok(1);
            }
        };
        if let Err(e) = instances.load(id) {
            writeln!(
                err,
                "failed to load instance: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
        content = Some(match Root::open(&game) {
            Ok(r) => r,
            Err(e) => {
                writeln!(err, "failed to open instance directory: {}", name(&e))?;
                return Ok(1);
            }
        });
    }
    let store = match crate::store::Store::open(store_directory) {
        Ok(r) => r,
        Err(e) => {
            writeln!(
                err,
                "failed to open artifact store: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let root = if let Some(r) = content {
        r
    } else {
        match Root::world(&game, world.unwrap(), true) {
            Ok(r) => r,
            Err(e) => {
                world_error(err, &e, "adopt")?;
                return Ok(1);
            }
        }
    };
    let adopted = match root.adopt(kind, &store) {
        Ok(a) => a,
        Err(e) => {
            if dp {
                world_error(err, &e, "adopt")?
            } else {
                writeln!(err, "failed to adopt unmanaged content: {}", name(&e))?
            }
            return Ok(1);
        }
    };
    for a in &adopted {
        out.write_all(b"adopted\t")?;
        out.write_all(&a.file.0)?;
        writeln!(out, "\t{}\t{}", a.sha1, a.size)?;
    }
    writeln!(out, "adoptedCount\t{}", adopted.len())?;
    Ok(0)
}

#[cfg(all(test, unix))]
mod native_error_tests {
    #[test]
    fn raw_name_error_keeps_content_boundary() {
        let error = super::Error::Io(std::io::Error::from_raw_os_error(
            rustix::io::Errno::ILSEQ.raw_os_error(),
        ));
        assert_eq!(super::name(&error), "BadPathName");
        assert_eq!(super::name(&super::Error::WorldLocked), "WorldLocked");
    }
}

/// A local CLI argument with one explicitly byte-preserving field.
pub(crate) enum LocalArg<'a> {
    Text(&'a str),
    File(Vec<u8>),
}
impl LocalArg<'_> {
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Text(v) => Some(v),
            Self::File(_) => None,
        }
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(v) => v.as_bytes(),
            Self::File(v) => v,
        }
    }
}
/// Narrow pre-dispatch hook for native instance-local path arguments. Server
/// interception is limited to local native server-root and mutation --file paths.
pub fn local_cli_os(
    args: &[std::ffi::OsString],
    out: &mut impl Write,
    errors: &mut impl Write,
) -> Option<io::Result<u8>> {
    let family = args.get(1)?.to_str()?;
    let kind = Kind::from_directory(family)?;
    let op = args.get(2)?.to_str()?;
    let server = match args.first()?.to_str()? {
        "instance" => false,
        "server" if matches!(family, "mods" | "datapacks") => true,
        _ => return None,
    };
    if !matches!(op, "list" | "enable" | "disable" | "remove") {
        return None;
    }
    // The source rejects unsupported platforms before inspecting local args.
    // Keep this gate confined to the four already-recognized local routes.
    #[cfg(not(target_os = "linux"))]
    if server {
        return Some(
            writeln!(
                errors,
                "Server management is currently supported only on Linux"
            )
            .map(|_| 1),
        );
    }
    // Invalid IDs precede all option parsing, including uninterpreted raw tails.
    // Forward the genuine argument alone to the existing rejecting parser.
    #[cfg(target_os = "linux")]
    if server
        && let Some(id) = args.get(3).filter(|id| {
            id.to_str()
                .is_none_or(|text| crate::instances::validate_id(text).is_err())
        })
    {
        return Some(crate::server_content::local_cli_bytes(
            family,
            op,
            &[match id.to_str() {
                Some(text) => LocalArg::Text(text),
                None => LocalArg::File(super::fs::argument_bytes(id)),
            }],
            out,
            errors,
        ));
    }
    let mut has_raw = false;
    let mut local = Vec::new();
    for (i, arg) in args.iter().enumerate().skip(3) {
        if let Some(text) = arg.to_str() {
            local.push(LocalArg::Text(text));
        } else {
            let previous = args.get(i.wrapping_sub(1))?.to_str();
            let allowed = previous == Some("--file") && op != "list"
                || server && matches!(previous, Some("--servers-directory" | "--project"))
                || !server
                    && (previous == Some("--instances-directory")
                        || kind == Kind::Datapack && previous == Some("--world"));
            if i == 3 || !allowed {
                return None;
            }
            has_raw = true;
            local.push(LocalArg::File(super::fs::argument_bytes(arg)));
        }
    }
    if !has_raw {
        return None;
    }
    if server {
        #[cfg(target_os = "linux")]
        return Some(crate::server_content::local_cli_bytes(
            family, op, &local, out, errors,
        ));
        #[cfg(not(target_os = "linux"))]
        unreachable!();
    }
    Some(local_inner(kind, op, &local, out, errors))
}

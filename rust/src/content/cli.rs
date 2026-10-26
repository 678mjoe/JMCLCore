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
    if !matches!(op, "list" | "enable" | "disable" | "remove") || args.is_empty() {
        err.write_all(usage(kind, op).as_bytes())?;
        return Ok(2);
    }
    let id = &args[0];
    if crate::instances::validate_id(id).is_err() {
        return bad(err);
    }
    let datapack = kind == Kind::Datapack;
    let family = kind.directory();
    let mut directory = "instances".to_string();
    let (mut world, mut project, mut file) = (None, None, None);
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "--json" && op == "list" {
            json = true;
            i += 1;
            continue;
        }
        let Some(v) = args.get(i + 1) else {
            return bad(err);
        };
        if v.is_empty() && !(op == "list" && !datapack && a == "--instances-directory") {
            return bad(err);
        }
        match a.as_str() {
            "--instances-directory" => directory = v.clone(),
            "--world" if datapack => world = Some(v.as_str()),
            "--project" if op != "list" => project = Some(v.as_str()),
            "--file" if op != "list" => file = Some(v.as_str()),
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
    let game = crate::instances::minecraft_path(&directory, id);
    let root = if datapack {
        match Root::world(&game, world.unwrap(), op != "list") {
            Ok(r) => r,
            Err(e) => {
                world_error(err, &e, op)?;
                return Ok(1);
            }
        }
    } else {
        let instances = match crate::instances::Root::open(&directory) {
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
        match Root::open(&game) {
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
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    kind.as_str().unwrap(),
                    e.file,
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
        match root.remove(kind, project, file) {
            Ok(e) => writeln!(out, "removed\t{}\t{}", e.file, e.sha1)?,
            Err(e) => {
                mutation_error(err, &e, kind, op)?;
                return Ok(1);
            }
        }
    } else {
        match root.toggle(kind, project, file, op == "enable") {
            Ok((changed, e)) => {
                let state = if op == "enable" {
                    "enabled"
                } else {
                    "disabled"
                };
                if changed {
                    writeln!(out, "{state}\t{}", e.file)?
                } else {
                    writeln!(out, "unchanged\t{}\t{state}", e.file)?
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
        writeln!(out, "adopted\t{}\t{}\t{}", a.file, a.sha1, a.size)?;
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

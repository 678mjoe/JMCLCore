//! `instance worlds` commands. The caller passes arguments after that prefix.
use super::*;
use std::io::Write;
const USAGE: &str = "usage: jmcl-core instance worlds <list|show|rename|duplicate|delete|export|import|backup|backups|restore|backups-delete> ID [ARGS] [--instances-directory PATH] [--json]\n";
fn usage(command: &str) -> String {
    let extra = match command {
        "list" | "backups" => "",
        "show" | "delete" | "backup" => " WORLD",
        "rename" | "duplicate" => " WORLD NEW_NAME",
        "export" => " WORLD FILE",
        "import" => " FILE",
        "restore" | "backups-delete" => " BACKUP",
        _ => return USAGE.into(),
    };
    let opts = match command {
        "list" | "show" | "backups" => " [--instances-directory PATH] [--json]",
        "import" | "restore" => " [--name NAME] [--replace] [--instances-directory PATH]",
        "backup" => " [--label LABEL] [--instances-directory PATH]",
        _ => " [--instances-directory PATH]",
    };
    format!("usage: jmcl-core instance worlds {command} ID{extra}{opts}\n")
}
struct Options<'a> {
    id: &'a str,
    world: Option<&'a str>,
    new: Option<&'a str>,
    name: Option<&'a str>,
    label: Option<&'a str>,
    directory: &'a str,
    json: bool,
    replace: bool,
}
fn parse(args: &[String]) -> Option<Options<'_>> {
    let id = args.first()?.as_str();
    crate::instances::validate_id(id).ok()?;
    let mut o = Options {
        id,
        world: None,
        new: None,
        name: None,
        label: None,
        directory: "instances",
        json: false,
        replace: false,
    };
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--json" => o.json = true,
            "--replace" => o.replace = true,
            "--instances-directory" | "--name" | "--label" if i + 1 < args.len() => {
                i += 1;
                let v = args[i].as_str();
                match a {
                    "--instances-directory" => o.directory = v,
                    "--name" => o.name = Some(v),
                    _ => o.label = Some(v),
                }
            }
            _ if o.world.is_none() => o.world = Some(a),
            _ if o.new.is_none() => o.new = Some(a),
            _ => return None,
        };
        i += 1
    }
    Some(o)
}
fn invalid(err: &mut impl Write) -> io::Result<u8> {
    err.write_all(b"error: InvalidArguments\n")?;
    Ok(1)
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("-")
}
// Display byte arrays verbatim, matching Zig's {s} writer. This helper is
// only used for the three world display strings, never request decoding.
fn display_bytes(out: &mut impl Write, v: &Value, key: &str) -> io::Result<()> {
    match v.get(key) {
        Some(Value::String(s)) => out.write_all(s.as_bytes()),
        Some(Value::Array(bytes)) => {
            for byte in bytes {
                let byte = byte.as_u64().filter(|b| *b <= 255).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid display byte")
                })?;
                out.write_all(&[byte as u8])?;
            }
            Ok(())
        }
        _ => out.write_all(b"-"),
    }
}
pub(super) fn row(out: &mut impl Write, v: &Value) -> io::Result<()> {
    for key in ["name", "level_name", "version_name"] {
        display_bytes(out, v, key)?;
        out.write_all(b"\t")?;
    }
    writeln!(
        out,
        "{}\t{}\t{}\t{}",
        text(v, "game_mode"),
        v["size_bytes"],
        if v["locked"] == true {
            "locked"
        } else {
            "idle"
        },
        text(v, "version_relation")
    )
}
pub(super) fn detail(out: &mut impl Write, v: &Value, meta: bool) -> io::Result<()> {
    out.write_all(b"name: ")?;
    display_bytes(out, v, "name")?;
    out.write_all(b"\n")?;
    if meta {
        out.write_all(b"level_name: ")?;
        display_bytes(out, v, "level_name")?;
        out.write_all(b"\nversion_name: ")?;
        display_bytes(out, v, "version_name")?;
        out.write_all(b"\n")?;
        for k in [
            "version_id",
            "game_mode",
            "hardcore",
            "cheats",
            "difficulty",
            "last_played_ms",
        ] {
            if !v[k].is_null() {
                if let Some(s) = v[k].as_str() {
                    writeln!(out, "{k}: {s}")?
                } else {
                    writeln!(out, "{k}: {}", v[k])?
                }
            }
        }
    } else {
        out.write_all(b"level.dat: unavailable\n")?
    }
    writeln!(
        out,
        "size_bytes: {}\nhas_icon: {}\nlocked: {}\nversion_relation: {}",
        v["size_bytes"],
        v["has_icon"],
        v["locked"],
        text(v, "version_relation")
    )
}
pub(super) fn report(out: &mut impl Write, operation: &str, e: Error) -> io::Result<u8> {
    let message = match e {
        Error::InvalidName => "invalid world name".into(),
        Error::WorldNotFound => "world not found".into(),
        Error::WorldExists => "a world with that name already exists".into(),
        Error::WorldLocked => "the world is in use by a running game".into(),
        Error::InvalidLevelData => "the world level.dat is malformed".into(),
        Error::UnsupportedEntry => "the world contains unsupported entries".into(),
        Error::BackupNotFound => "backup file not found".into(),
        Error::ArchiveTooLarge => "the archive exceeds the supported size limits".into(),
        Error::ArchiveInvalid => "the archive is not a valid world zip".into(),
        Error::Io(e) => crate::instances::cli_error_name(&e),
        Error::TooManyWorlds => "TooManyWorlds".into(),
        Error::IconTooLarge => "IconTooLarge".into(),
        Error::IconIo => "Unexpected".into(),
        Error::LevelDataMissing => "LevelDataMissing".into(),
    };
    writeln!(out, "failed to {operation} world: {message}")?;
    Ok(1)
}
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Option<io::Result<u8>> {
    Some(run(args, out, err))
}
fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    let command = args.first().map(String::as_str).unwrap_or("");
    if !matches!(
        command,
        "list"
            | "show"
            | "rename"
            | "duplicate"
            | "delete"
            | "export"
            | "import"
            | "backup"
            | "backups"
            | "restore"
            | "backups-delete"
    ) {
        err.write_all(USAGE.as_bytes())?;
        return Ok(2);
    }
    if args.len() == 1 {
        err.write_all(usage(command).as_bytes())?;
        return Ok(2);
    }
    let Some(o) = parse(&args[1..]) else {
        return invalid(err);
    };
    if matches!(command, "list" | "backups") && o.world.is_some() {
        return invalid(err);
    }
    if !matches!(command, "list" | "backups") && o.world.is_none() {
        return invalid(err);
    }
    if matches!(command, "show" | "delete") && o.new.is_some() {
        return invalid(err);
    }
    if matches!(command, "rename" | "duplicate" | "export") && o.new.is_none() {
        return invalid(err);
    }
    let root = match crate::instances::Root::open(o.directory) {
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
    // Only list/show use the byte presentation view. Mutation/archive branches
    // retain the original operational String manifest and version argument.
    let loaded_version = if matches!(command, "list" | "show") {
        root.load_presentation_version(o.id)
            .map(|v| (None, Some(v)))
    } else {
        root.load(o.id)
            .map(|manifest| (Some(manifest.version_id), None))
    };
    let (version_id, read_version) = match loaded_version {
        Ok(m) => m,
        Err(e) => {
            writeln!(
                err,
                "failed to load instance: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let path = crate::instances::minecraft_path(o.directory, o.id);
    let version = version_id.as_deref();
    let read_version = read_version.as_deref();
    let name = o.world.unwrap_or("");
    let operation = match command {
        "show" => "inspect",
        "backup" => "back up",
        "backups" => "list backups for",
        "backups-delete" => "delete backup",
        c => c,
    };
    let result = match command {
        "list" => read::list_read(Path::new(&path), read_version),
        "show" => saves(&path)
            .and_then(|s| read::world_read(&s, name.as_bytes(), read_version))
            .map(|w| json!({"world":w})),
        "rename" => rename(&path, name, o.new.unwrap(), version),
        "duplicate" => duplicate(&path, name, o.new.unwrap(), version),
        "delete" => delete(&path, name),
        "export" => archive::export(&path, name, o.new.unwrap()),
        "import" => archive::import(&path, name, o.name, o.replace, version),
        "backup" => archive::backup(&path, name, o.label),
        "backups" => backups(&path),
        "restore" => archive::restore(&path, name, o.name, o.replace, version),
        "backups-delete" => delete_backup(&path, name),
        _ => unreachable!(),
    };
    let result = match result {
        Ok(v) => v,
        Err(e) => return report(err, operation, e),
    };
    match command {
        "list" => {
            if o.json {
                serde_json::to_writer(&mut *out, &json!({"worlds":result["worlds"]}))?;
                writeln!(out)?
            } else {
                for w in result["worlds"].as_array().unwrap() {
                    row(out, w)?
                }
            }
        }
        "show" => {
            if o.json {
                serde_json::to_writer(&mut *out, &result)?;
                writeln!(out)?
            } else {
                let meta = saves(&path)
                    .ok()
                    .and_then(|s| s.child_alias(name).ok())
                    .and_then(|d| d.read_alias("level.dat", 32 * 1024 * 1024).ok())
                    .and_then(|b| decode_level(&b).ok())
                    .is_some_and(|d| matches!(d.root.get("Data"), Some(nbt::Tag::Compound(_))));
                detail(out, &result["world"], meta)?
            }
        }
        "rename" => writeln!(out, "renamed {name} -> {}", o.new.unwrap())?,
        "duplicate" => writeln!(out, "duplicated {name} -> {}", o.new.unwrap())?,
        "delete" | "backups-delete" => writeln!(out, "deleted {name}")?,
        "export" => writeln!(
            out,
            "exported {name} -> {} ({} bytes, {} files)",
            o.new.unwrap(),
            result["size_bytes"],
            result["files"]
        )?,
        "import" => writeln!(out, "imported {}", text(&result["world"], "name"))?,
        "restore" => writeln!(out, "restored {}", text(&result["world"], "name"))?,
        "backup" => writeln!(out, "backup: {}", text(&result, "backup"))?,
        "backups" => {
            if o.json {
                serde_json::to_writer(&mut *out, &json!({"backups":result["backups"]}))?;
                writeln!(out)?
            } else {
                for b in result["backups"].as_array().unwrap() {
                    writeln!(
                        out,
                        "{}\t{}\t{}\t{}",
                        text(b, "file"),
                        text(b, "world"),
                        b["created_ms"],
                        b["size_bytes"]
                    )?
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(0)
}

#[cfg(all(test, unix))]
mod native_error_tests {
    #[test]
    fn raw_name_error_keeps_world_operation_message() {
        let error = super::Error::Io(std::io::Error::from_raw_os_error(
            rustix::io::Errno::ILSEQ.raw_os_error(),
        ));
        let mut out = Vec::new();
        assert_eq!(super::report(&mut out, "import", error).unwrap(), 1);
        assert_eq!(out, b"failed to import world: BadPathName\n");
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn byte_display_preserves_string_null_empty_and_rejects_invalid_arrays() {
        for (value, expected) in [
            (json!("世界"), "世界".as_bytes().to_vec()),
            (Value::Null, b"-".to_vec()),
            (json!(""), vec![]),
            (json!([255, 0, 10]), vec![255, 0, 10]),
        ] {
            let mut out = vec![];
            display_bytes(&mut out, &json!({"field": value}), "field").unwrap();
            assert_eq!(out, expected);
        }
        for value in [json!([256]), json!([-1]), json!(["65"])] {
            assert_eq!(
                display_bytes(&mut vec![], &json!({"field":value}), "field")
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut out = vec![];
        row(&mut out, &json!({"name":"world","level_name":null,"version_name":"","game_mode":"creative","size_bytes":3,"locked":false,"version_relation":"unknown"})).unwrap();
        assert_eq!(out, b"world\t-\t\tcreative\t3\tidle\tunknown\n");
        out.clear();
        detail(&mut out, &json!({"name":"world","level_name":null,"version_name":"","size_bytes":3,"has_icon":false,"locked":false,"version_relation":"unknown"}), true).unwrap();
        assert_eq!(out, b"name: world\nlevel_name: -\nversion_name: \nsize_bytes: 3\nhas_icon: false\nlocked: false\nversion_relation: unknown\n");
    }
}

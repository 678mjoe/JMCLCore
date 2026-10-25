//! Server-world CLI, preserving the original positional/option grammar.
use super::*;
use std::io::Write;
const USAGE: &str = "usage: jmcl-core server worlds <list|show|rename|delete|backup|backups|restore|backups-delete> ID [ARGS] [--servers-directory PATH] [--json]";
fn usage(command: &str) -> String {
    let (positionals, opts) = match command {
        "list" | "backups" => ("", " [--servers-directory PATH] [--json]"),
        "show" => (" WORLD", " [--servers-directory PATH] [--json]"),
        "rename" => (" WORLD NEW_NAME", " [--servers-directory PATH]"),
        "delete" => (" WORLD", " [--servers-directory PATH]"),
        "backup" => (" [WORLD]", " [--label LABEL] [--servers-directory PATH]"),
        "restore" => (" BACKUP", " [--replace] [--servers-directory PATH]"),
        "backups-delete" => (" BACKUP", " [--servers-directory PATH]"),
        _ => return USAGE.into(),
    };
    format!("usage: jmcl-core server worlds {command} ID{positionals}{opts}")
}
fn invalid(err: &mut impl Write) -> io::Result<u8> {
    writeln!(err, "error: InvalidArguments")?;
    Ok(1)
}
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    let command = args.first().map(String::as_str).unwrap_or("");
    if !matches!(
        command,
        "list" | "show" | "rename" | "delete" | "backup" | "backups" | "restore" | "backups-delete"
    ) {
        writeln!(err, "{USAGE}")?;
        return Ok(2);
    }
    let args = &args[1..];
    let minimum = match command {
        "rename" => 3,
        "delete" | "restore" | "backups-delete" => 2,
        _ => 1,
    };
    if args.len() < minimum {
        writeln!(err, "{}", usage(command))?;
        return Ok(2);
    }
    let mut directory = "servers";
    let mut world = None;
    let mut new = None;
    let mut label = None;
    let mut replace = false;
    let mut json_output = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json_output = true,
            "--replace" => replace = true,
            "--servers-directory" | "--label" if i + 1 < args.len() => {
                let option = &args[i];
                i += 1;
                if option == "--servers-directory" {
                    directory = &args[i];
                } else {
                    label = Some(args[i].as_str());
                }
            }
            _ if world.is_none() => world = Some(args[i].as_str()),
            _ if new.is_none() => new = Some(args[i].as_str()),
            _ => return invalid(err),
        }
        i += 1;
    }
    if matches!(command, "list" | "backups") && world.is_some()
        || matches!(
            command,
            "show" | "rename" | "delete" | "restore" | "backups-delete"
        ) && world.is_none()
        || command == "rename" && new.is_none()
    {
        return invalid(err);
    }
    if !cfg!(target_os = "linux")
        || matches!(command, "list" | "show" | "backups" | "backups-delete")
    {
        return super::server_portable_cli::run(
            command,
            directory,
            &args[0],
            world,
            json_output,
            out,
            err,
        );
    }
    #[cfg(target_os = "linux")]
    {
        let method = format!(
            "server.worlds.{}",
            match command {
                "show" => "get",
                "backups-delete" => "backups.delete",
                _ => command,
            }
        );
        let mut params = json!({"directory":directory,"id":args[0]});
        if let Some(world) = world {
            params[if matches!(command, "restore" | "backups-delete") {
                "backup"
            } else {
                "world"
            }] = world.into();
        }
        if let Some(new) = new {
            params["new_name"] = new.into();
        }
        if let Some(label) = label {
            params["label"] = label.into();
        }
        params["replace"] = replace.into();
        let raw = serde_json::value::to_raw_value(&params)?;
        let result = match super::server::handle(&method, Some(&raw)).unwrap() {
            Ok(v) => v,
            Err(e) => {
                let operation = match command {
                    "list" => "list",
                    "show" => "inspect",
                    "backup" => "back up",
                    "backups" => "list backups for",
                    "backups-delete" => "delete backup",
                    _ => command,
                };
                let message = match e.code.as_str() {
                    "INVALID_PARAMS" => "invalid world name",
                    "WORLD_NOT_FOUND" => "world not found",
                    "WORLD_EXISTS" => "a world with that name already exists",
                    "WORLD_LOCKED" => "the world is in use by a running game",
                    "WORLD_INVALID_LEVEL_DATA" => "the world level.dat is malformed",
                    "BACKUP_NOT_FOUND" => "backup file not found",
                    "WORLD_ARCHIVE_TOO_LARGE" => "the archive exceeds the supported size limits",
                    "WORLD_ARCHIVE_INVALID" => "the archive is not a valid world zip",
                    "SERVER_NOT_FOUND" => "server not found",
                    "SERVER_ALREADY_RUNNING" => "stop the server before modifying its worlds",
                    _ => "Unexpected",
                };
                writeln!(err, "failed to {operation} world: {message}")?;
                return Ok(1);
            }
        };
        match command {
            "list" => {
                if json_output {
                    serde_json::to_writer(&mut *out, &json!({"worlds":result["worlds"]}))?;
                    writeln!(out)?;
                } else {
                    for w in result["worlds"].as_array().unwrap() {
                        super::cli::row(out, w)?;
                    }
                }
            }
            "show" => {
                if json_output {
                    serde_json::to_writer(&mut *out, &json!({"world":result["world"]}))?;
                    writeln!(out)?;
                } else {
                    let meta = crate::servers::store::open_root(directory)
                        .ok()
                        .and_then(|r| crate::servers::store::open_server(&r, &args[0]).ok())
                        .map(fs::Dir)
                        .and_then(|d| d.child(world.unwrap(), false).ok())
                        .and_then(|d| d.read_alias("level.dat", 32 * 1024 * 1024).ok())
                        .and_then(|b| decode_level(&b).ok())
                        .is_some_and(|d| matches!(d.root.get("Data"), Some(nbt::Tag::Compound(_))));
                    super::cli::detail(out, &result["world"], meta)?;
                }
            }
            "rename" => writeln!(out, "renamed {} -> {}", world.unwrap(), new.unwrap())?,
            "delete" | "backups-delete" => writeln!(out, "deleted {}", world.unwrap())?,
            "backup" => writeln!(out, "backup: {}", result["backup"].as_str().unwrap())?,
            "restore" => writeln!(out, "restored {}", world.unwrap())?,
            "backups" => {
                if json_output {
                    serde_json::to_writer(&mut *out, &json!({"backups":result["backups"]}))?;
                    writeln!(out)?;
                } else {
                    for b in result["backups"].as_array().unwrap() {
                        writeln!(
                            out,
                            "{}\t{}\t{}\t{}",
                            b["file"].as_str().unwrap(),
                            b["world"].as_str().unwrap_or("-"),
                            b["created_ms"],
                            b["size_bytes"]
                        )?;
                    }
                }
            }
            _ => unreachable!(),
        }
        Ok(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (new, label, replace);
        unreachable!()
    }
}

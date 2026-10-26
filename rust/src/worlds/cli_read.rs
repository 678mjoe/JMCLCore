//! Narrow native-argv adapter for world list/show; all UTF-8 argv stays on the
//! existing CLI. No mutation or archive command is recognized here.
use super::*;
use std::ffi::{OsStr, OsString};

pub fn read_cli_os(
    args: &[OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<io::Result<u8>> {
    let server = match args.first()?.to_str()? {
        "instance" => false,
        "server" => true,
        _ => return None,
    };
    if args.get(1)? != "worlds" {
        return None;
    }
    let command = args.get(2)?.to_str()?;
    if !matches!(command, "list" | "show") || args.iter().all(|a| a.to_str().is_some()) {
        return None;
    }
    Some(run(server, command, &args[3..], out, err))
}
fn invalid(err: &mut impl Write) -> io::Result<u8> {
    writeln!(err, "error: InvalidArguments")?;
    Ok(1)
}
fn valid_id(id: &OsStr) -> Option<&str> {
    id.to_str()
        .filter(|id| crate::instances::validate_id(id).is_ok())
}
fn run(
    server: bool,
    command: &str,
    args: &[OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    // A recognized raw-argv route necessarily has an argument after the prefix.
    let Some(id) = args.first() else {
        return invalid(err);
    };
    if !server && valid_id(id).is_none() {
        return invalid(err);
    }
    let directory_option = if server {
        "--servers-directory"
    } else {
        "--instances-directory"
    };
    let mut directory = OsStr::new(if server { "servers" } else { "instances" });
    let mut world: Option<&OsStr> = None;
    let mut new: Option<&OsStr> = None;
    let mut json_output = false;
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_os_str();
        if arg == "--json" {
            json_output = true;
        } else if arg == "--replace" {
            // Recognized but unused for these read-only commands.
        } else if (arg == directory_option || arg == "--label" || (!server && arg == "--name"))
            && i + 1 < args.len()
        {
            i += 1;
            if arg == directory_option {
                directory = &args[i];
            }
        } else if world.is_none() {
            world = Some(arg);
        } else if new.is_none() {
            new = Some(arg);
        } else {
            return invalid(err);
        }
        i += 1;
    }
    if command == "list" && world.is_some()
        || command == "show" && (world.is_none() || (!server && new.is_some()))
    {
        return invalid(err);
    }
    if server {
        server_read(command, directory, id, world, json_output, out, err)
    } else {
        instance_read(
            command,
            directory,
            valid_id(id).unwrap(),
            world,
            json_output,
            out,
            err,
        )
    }
}
fn render(
    command: &str,
    result: &Value,
    json_output: bool,
    has_metadata: impl FnOnce() -> bool,
    out: &mut impl Write,
) -> io::Result<u8> {
    if json_output {
        serde_json::to_writer(&mut *out, result)?;
        writeln!(out)?;
    } else if command == "list" {
        for world in result["worlds"].as_array().unwrap() {
            cli::row(out, world)?;
        }
    } else {
        cli::detail(out, &result["world"], has_metadata())?;
    }
    Ok(0)
}
fn metadata(dir: Result<fs::Dir>) -> bool {
    dir.ok()
        .and_then(|d| d.read_alias("level.dat", 32 * 1024 * 1024).ok())
        .and_then(|b| decode_level(&b).ok())
        .is_some_and(|d| matches!(d.root.get("Data"), Some(nbt::Tag::Compound(_))))
}
fn instance_read(
    command: &str,
    directory: &OsStr,
    id: &str,
    world: Option<&OsStr>,
    json_output: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let root = match crate::instances::Root::open_native(directory) {
        Ok(root) => root,
        Err(e) => {
            writeln!(
                err,
                "failed to open instances directory: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let manifest = match root.load_read(id) {
        Ok(manifest) => manifest,
        Err(e) => {
            writeln!(
                err,
                "failed to load instance: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let path = Path::new(directory).join(id).join(".minecraft");
    let version = Some(manifest.version_id.as_bytes());
    let result = if command == "list" {
        read::list_read(&path, version).map(|result| json!({"worlds": result["worlds"]}))
    } else {
        read::saves_read(&path)
            .and_then(|dir| {
                read::world_read(&dir, &read::listed_name_bytes(world.unwrap()), version)
            })
            .map(|world| json!({"world": world}))
    };
    let result = match result {
        Ok(result) => result,
        Err(e) => return cli::report(err, if command == "show" { "inspect" } else { "list" }, e),
    };
    render(
        command,
        &result,
        json_output,
        || metadata(read::saves_read(&path).and_then(|dir| dir.child_alias_native(world.unwrap()))),
        out,
    )
}
fn server_read(
    command: &str,
    directory: &OsStr,
    id: &OsStr,
    world: Option<&OsStr>,
    json_output: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    use super::server_portable_cli as portable;
    let root = match portable::root_native(directory) {
        Ok(root) => root,
        Err(e) => {
            writeln!(
                err,
                "failed to open servers directory: {}",
                portable::name(e)
            )?;
            return Ok(1);
        }
    };
    let Some(id) = valid_id(id) else {
        writeln!(err, "failed to load server: InvalidServerId")?;
        return Ok(1);
    };
    let dir = match portable::load_read(&root, id) {
        Ok(dir) => dir,
        Err(e) => {
            writeln!(err, "failed to load server: {}", portable::name(e))?;
            return Ok(1);
        }
    };
    let result = if command == "list" {
        portable::list_read(&dir)
    } else {
        read::world_read(&dir, &read::listed_name_bytes(world.unwrap()), None)
            .map(|world| json!({"world":world}))
    };
    let result = match result {
        Ok(result) => result,
        Err(e) => {
            return portable::report(err, if command == "show" { "inspect" } else { "list" }, e);
        }
    };
    // Preserve the preexisting CLI metadata-presence lookup policy separately
    // from the named world metadata read, without broadening alias behavior.
    render(
        command,
        &result,
        json_output,
        || metadata(dir.child(world.unwrap(), false)),
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opaque() -> OsString {
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
    fn unrelated_routes_and_utf8_never_execute_or_write() {
        // Dispatch-only checks: these operations must never run in this test.
        for prefix in [
            vec!["instance", "worlds", "rename"],
            vec!["instance", "worlds", "export"],
            vec!["server", "worlds", "backup"],
            vec!["server", "worlds", "restore"],
            vec!["instance", "mods", "list"],
            vec!["__jmcl_server_supervisor"],
            vec!["rpc"],
        ] {
            let mut args: Vec<OsString> = prefix.into_iter().map(Into::into).collect();
            args.push(opaque());
            let (mut out, mut err) = (vec![], vec![]);
            assert!(read_cli_os(&args, &mut out, &mut err).is_none());
            assert!(out.is_empty() && err.is_empty());
        }
        for domain in ["instance", "server"] {
            for command in ["list", "show"] {
                let args: Vec<OsString> = [domain, "worlds", command, "demo", "Alpha"]
                    .into_iter()
                    .map(Into::into)
                    .collect();
                let (mut out, mut err) = (vec![], vec![]);
                assert!(read_cli_os(&args, &mut out, &mut err).is_none());
                assert!(out.is_empty() && err.is_empty());
            }
        }
    }
}

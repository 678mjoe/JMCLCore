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
    let version_id = match root.load_presentation_version(id) {
        Ok(version) => version,
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
    let version = Some(version_id.as_slice());
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
    let mut show_has_metadata = false;
    let result = if command == "list" {
        portable::list_read(&dir)
    } else {
        read::world_read_with_metadata(&dir, &read::listed_name_bytes(world.unwrap()), None).map(
            |(world, has_metadata)| {
                show_has_metadata = has_metadata;
                json!({"world":world})
            },
        )
    };
    let result = match result {
        Ok(result) => result,
        Err(e) => {
            return portable::report(err, if command == "show" { "inspect" } else { "list" }, e);
        }
    };
    render(command, &result, json_output, || show_has_metadata, out)
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

#[cfg(all(test, unix))]
mod version_bridge_tests {
    use super::*;
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    type Snapshot =
        std::collections::BTreeMap<std::path::PathBuf, (Vec<u8>, u64, u64, u64, u32, i64, i64)>;
    fn snapshot(root: &Path) -> Snapshot {
        fn walk(root: &Path, path: &Path, out: &mut Snapshot) {
            let m = std::fs::symlink_metadata(path).unwrap();
            let bytes = if m.is_file() {
                std::fs::read(path).unwrap()
            } else {
                vec![]
            };
            out.insert(
                path.strip_prefix(root).unwrap().into(),
                (
                    bytes,
                    m.dev(),
                    m.ino(),
                    m.nlink(),
                    m.mode(),
                    m.mtime(),
                    m.mtime_nsec(),
                ),
            );
            if m.is_dir() {
                for item in std::fs::read_dir(path).unwrap() {
                    walk(root, &item.unwrap().path(), out);
                }
            }
        }
        let mut out = Snapshot::new();
        walk(root, root, &mut out);
        out
    }
    fn fixture(raw_path: bool) -> (tempfile::TempDir, std::path::PathBuf) {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join(if raw_path {
            OsStr::from_bytes(b"instances-\xff")
        } else {
            OsStr::new("instances")
        });
        std::fs::create_dir_all(root.join("demo/.minecraft/saves")).unwrap();
        std::fs::write(
            root.join("demo/instance.json"),
            br#"{"id":"demo","name":[255],"version_id":[254],"fabric_loader":[253]}"#,
        )
        .unwrap();
        std::fs::write(owner.path().join("sentinel"), b"unchanged").unwrap();
        std::fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
        (owner, root)
    }
    fn check(raw_path: bool) {
        let (owner, root) = fixture(raw_path);
        let before = snapshot(owner.path());
        // Saves is strictly empty: no level.dat, archives, cleanup markers or game files.
        assert_eq!(
            std::fs::read_dir(root.join("demo/.minecraft/saves"))
                .unwrap()
                .count(),
            0
        );
        for (command, world, expected_code, expected_out, expected_err) in [
            (
                "list",
                None,
                0,
                b"{\"worlds\":[]}\n".as_slice(),
                b"".as_slice(),
            ),
            (
                "show",
                Some("missing"),
                1,
                b"".as_slice(),
                b"failed to inspect world: world not found\n".as_slice(),
            ),
        ] {
            assert!(matches!(command, "list" | "show"));
            assert!(root.is_dir() && root.starts_with(owner.path()));
            let mut args = vec![OsString::from(command), OsString::from("demo")];
            if let Some(world) = world {
                args.push(world.into());
            }
            args.extend([
                "--instances-directory".into(),
                root.as_os_str().to_owned(),
                "--json".into(),
            ]);
            let (mut out, mut err) = (vec![], vec![]);
            let code = if raw_path {
                let mut full = vec!["instance".into(), "worlds".into()];
                full.extend(args);
                read_cli_os(&full, &mut out, &mut err).unwrap().unwrap()
            } else {
                let strings = args
                    .iter()
                    .map(|a| a.to_str().unwrap().to_owned())
                    .collect::<Vec<_>>();
                cli::cli(&strings, &mut out, &mut err).unwrap().unwrap()
            };
            assert_eq!(
                (code, out, err),
                (expected_code, expected_out.to_vec(), expected_err.to_vec()),
                "{command}, raw={raw_path}"
            );
            assert_eq!(snapshot(owner.path()), before);
        }
    }
    #[test]
    fn utf8_empty_saves_and_missing_world_preserve_owned_tree() {
        check(false);
    }
    #[test]
    fn native_empty_saves_and_missing_world_preserve_owned_tree() {
        check(true);
    }
    #[test]
    fn native_read_adapter_excludes_mutation_and_archive_routes() {
        for command in [
            "rename",
            "duplicate",
            "delete",
            "export",
            "import",
            "backup",
            "backups",
            "restore",
            "backups-delete",
        ] {
            let args = vec![
                "instance".into(),
                "worlds".into(),
                command.into(),
                "demo".into(),
                OsStr::from_bytes(b"unused-\xff").to_owned(),
            ];
            let (mut out, mut err) = (vec![], vec![]);
            assert!(read_cli_os(&args, &mut out, &mut err).is_none());
            assert!(out.is_empty() && err.is_empty());
        }
    }
}

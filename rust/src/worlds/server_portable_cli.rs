//! Server-world CLI paths that the Zig baseline permits without Linux lifecycle.
//! This does not relax the protocol's Linux platform gate.
use super::*;
use crate::servers::{manifest::Manifest, properties};
#[cfg(unix)]
use std::fs::File;
use std::io::Write;
fn named(s: &str) -> Error {
    Error::Io(io::Error::new(io::ErrorKind::InvalidData, s.to_owned()))
}
fn root(path: &str) -> Result<fs::Dir> {
    if path.is_empty() {
        return Err(named("EmptyServersRoot"));
    }
    #[cfg(unix)]
    {
        use rustix::fs::{Mode, OFlags, open};
        std::fs::create_dir_all(path)?;
        Ok(fs::Dir(File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io::Error::from)?,
        )))
    }
    #[cfg(windows)]
    {
        Ok(fs::Dir(crate::store::windows::walk(Path::new(path), true)?))
    }
}
fn load(root: &fs::Dir, id: &str) -> Result<fs::Dir> {
    crate::instances::validate_id(id).map_err(|_| named("InvalidServerId"))?;
    let dir = root.child(id, false)?;
    let bytes = dir.read_alias("server.json", 64 * 1024).map_err(|e| {
        if matches!(e, Error::ArchiveTooLarge) {
            named("StreamTooLong")
        } else {
            e
        }
    })?;
    Manifest::parse(&bytes, id).map_err(named)?;
    Ok(dir)
}
fn name(e: Error) -> String {
    match e {
        Error::Io(e) => crate::instances::cli_error_name(&e),
        Error::UnsupportedEntry => "NotFile".into(),
        _ => "Unexpected".into(),
    }
}
fn checked(path: &str, id: &str) -> Result<fs::Dir> {
    let r = root(path).map_err(|_| named("ServerNotFound"))?;
    load(&r, id).map_err(|e| match &e {
        Error::Io(io)
            if io.kind() == io::ErrorKind::NotFound || io.to_string() == "InvalidServerId" =>
        {
            named("ServerNotFound")
        }
        _ => e,
    })
}
fn report(out: &mut impl Write, operation: &str, e: Error) -> io::Result<u8> {
    if let Error::Io(ref io) = e
        && io.to_string() == "ServerNotFound"
    {
        writeln!(out, "failed to {operation} world: server not found")?;
        return Ok(1);
    }
    super::cli::report(out, operation, e)
}
pub(super) fn run(
    command: &str,
    directory: &str,
    id: &str,
    world_name: Option<&str>,
    json_output: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let operation = match command {
        "list" => "list",
        "show" => "inspect",
        "backups" => "list backups for",
        "backups-delete" => "delete backup",
        "backup" => "back up",
        other => other,
    };
    if command == "restore"
        && let Err(e) = validate_backup(world_name.unwrap())
    {
        return report(err, operation, e);
    }
    let dir = if matches!(command, "list" | "show") {
        let r = match root(directory) {
            Ok(r) => r,
            Err(e) => {
                writeln!(err, "failed to open servers directory: {}", name(e))?;
                return Ok(1);
            }
        };
        match load(&r, id) {
            Ok(d) => d,
            Err(e) => {
                writeln!(err, "failed to load server: {}", name(e))?;
                return Ok(1);
            }
        }
    } else {
        match checked(directory, id) {
            Ok(d) => d,
            Err(e) => return report(err, operation, e),
        }
    };
    let result = (|| -> Result<Value> {
        match command {
            "list" => {
                let level = dir
                    .read_alias("server.properties", properties::MAX_FILE_BYTES)
                    .ok()
                    .and_then(|b| properties::parse(&b).ok())
                    .and_then(|d| d.get("level-name").map(str::to_owned))
                    .unwrap_or_else(|| "world".into());
                let mut worlds = vec![];
                for n in [
                    level.clone(),
                    format!("{level}_nether"),
                    format!("{level}_the_end"),
                ] {
                    if validate_name(&n).is_err() {
                        continue;
                    }
                    match super::world(&dir, &n, None) {
                        Ok(w) => worlds.push(w),
                        Err(Error::WorldNotFound) => {}
                        Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotADirectory => {}
                        Err(e) => return Err(e),
                    }
                }
                Ok(json!({"worlds":worlds}))
            }
            "show" => Ok(json!({"world":super::world(&dir,world_name.unwrap(),None)?})),
            "backups" => {
                let mut v = backups_in(&dir)?;
                v.as_object_mut().unwrap().remove("stage");
                Ok(v)
            }
            "backups-delete" => {
                let backup = dir.child("backups", false).map_err(|e| {
                    if matches!(&e,Error::Io(e) if e.kind()==io::ErrorKind::NotFound) {
                        Error::BackupNotFound
                    } else {
                        e
                    }
                })?;
                let name = world_name.unwrap();
                validate_backup(name)?;
                backup.unlink(name).map_err(|e| {
                    if matches!(&e,Error::Io(e) if e.kind()==io::ErrorKind::NotFound) {
                        Error::BackupNotFound
                    } else {
                        e
                    }
                })?;
                Ok(Value::Null)
            }
            // Non-Linux mutation requests reach lifecycle only after opening the
            // checked server, exactly like the original service methods.
            _ => Err(named("UnsupportedPlatform")),
        }
    })();
    let result = match result {
        Ok(v) => v,
        Err(Error::Io(e)) if e.to_string() == "UnsupportedPlatform" => {
            writeln!(
                err,
                "failed to {operation} world: server world management is only supported on Linux"
            )?;
            return Ok(1);
        }
        Err(e) => return report(err, operation, e),
    };
    if json_output && command != "backups-delete" {
        serde_json::to_writer(&mut *out, &result)?;
        writeln!(out)?;
        return Ok(0);
    }
    match command {
        "list" => {
            for w in result["worlds"].as_array().unwrap() {
                super::cli::row(out, w)?
            }
        }
        "show" => {
            let has_metadata = dir
                .child(world_name.unwrap(), false)
                .ok()
                .and_then(|d| d.read_alias("level.dat", 32 * 1024 * 1024).ok())
                .and_then(|b| decode_level(&b).ok())
                .is_some_and(|d| matches!(d.root.get("Data"), Some(nbt::Tag::Compound(_))));
            super::cli::detail(out, &result["world"], has_metadata)?
        }
        "backups" => {
            for b in result["backups"].as_array().unwrap() {
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}",
                    b["file"].as_str().unwrap(),
                    b["world"].as_str().unwrap_or("-"),
                    b["created_ms"],
                    b["size_bytes"]
                )?
            }
        }
        "backups-delete" => writeln!(out, "deleted {}", world_name.unwrap())?,
        _ => unreachable!(),
    }
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_listing_and_backup_removal_do_not_create_lifecycle_state() {
        let temp = tempfile::tempdir().unwrap();
        let server = temp.path().join("demo");
        std::fs::create_dir_all(server.join("custom")).unwrap();
        std::fs::write(
            server.join("server.json"),
            br#"{"id":"demo","name":"Demo","version_id":"fixture"}"#,
        )
        .unwrap();
        std::fs::write(server.join("server.properties"), b"level-name=custom\n").unwrap();
        let mut out = vec![];
        let mut err = vec![];
        assert_eq!(
            run(
                "list",
                temp.path().to_str().unwrap(),
                "demo",
                None,
                true,
                &mut out,
                &mut err
            )
            .unwrap(),
            0
        );
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["worlds"][0]["name"], "custom");
        assert!(!server.join(".jmcl").exists());
        std::fs::create_dir(server.join("backups")).unwrap();
        std::fs::write(server.join("backups/fixture.zip"), b"fixture").unwrap();
        assert_eq!(
            run(
                "backups-delete",
                temp.path().to_str().unwrap(),
                "demo",
                Some("fixture.zip"),
                false,
                &mut vec![],
                &mut err
            )
            .unwrap(),
            0
        );
        assert!(!server.join("backups/fixture.zip").exists());
        assert!(!server.join(".jmcl").exists());
    }
    #[test]
    fn portable_mutation_stub_preserves_open_and_validation_before_platform_error() {
        let temp = tempfile::tempdir().unwrap();
        let mut err = vec![];
        assert_eq!(
            run(
                "rename",
                temp.path().to_str().unwrap(),
                "absent",
                Some("world"),
                false,
                &mut vec![],
                &mut err
            )
            .unwrap(),
            1
        );
        assert_eq!(err, b"failed to rename world: server not found\n");
        let mut err = vec![];
        assert_eq!(
            run(
                "restore",
                temp.path().to_str().unwrap(),
                "absent",
                Some("invalid"),
                false,
                &mut vec![],
                &mut err
            )
            .unwrap(),
            1
        );
        assert_eq!(err, b"failed to restore world: invalid world name\n");
    }
}

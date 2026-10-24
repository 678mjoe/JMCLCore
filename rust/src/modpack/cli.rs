use super::{Error, provider, rpc, service};
use crate::install;
use std::io::{self, Write};
/// Arguments after `instance modpack`.
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    if args.first().map(String::as_str) != Some("install") {
        err.write_all(b"usage: jmcl-core instance modpack <install> ID [options]\n")?;
        return Ok(2);
    }
    let Some(id) = args.get(1) else {
        err.write_all(b"usage: jmcl-core instance modpack install ID --file PATH [--api-key KEY] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]\n")?;
        return Ok(2);
    };

    if crate::instances::validate_id(id).is_err() {
        return bad(err);
    }
    let mut file = None;
    let mut key = None;
    let mut directory = "instances";
    let mut store = ".jmclcore";
    let mut workers = 8;
    let mut retries = 2;
    let mut at = 2;
    while at < args.len() {
        let Some(value) = args.get(at + 1).filter(|v| !v.is_empty()) else {
            return bad(err);
        };
        match args[at].as_str() {
            "--file" => file = Some(value.as_str()),
            "--api-key" => key = Some(value.as_str()),
            "--instances-directory" => directory = value,
            "--store-directory" => store = value,
            "--workers" => {
                let Ok(v) = value.parse::<u8>() else {
                    return bad(err);
                };
                if v == 0 || v > 32 {
                    return bad(err);
                }
                workers = v
            }
            "--retries" => {
                let Ok(v) = value.parse::<u8>() else {
                    return bad(err);
                };
                if v > 5 {
                    return bad(err);
                }
                retries = v
            }
            _ => return bad(err),
        }
        at += 2;
    }
    let Some(file) = file else {
        err.write_all(b"instance modpack install requires --file\n")?;
        return Ok(2);
    };
    let mut last = 0;
    let result = rpc::stream(
        |sink| {
            let api = provider::HttpApi::new()?;
            let transport = install::HttpTransport::new()?;
            let mut provider = provider::Provider {
                transport: api,
                key: key.unwrap_or(""),
            };
            service::install(
                service::Request {
                    directory,
                    store_directory: store,
                    id,
                    file,
                    api_key: key,
                },
                install::Options {
                    workers,
                    retries,
                    sync_files: true,
                },
                &mut provider,
                &transport,
                Some(sink),
            )
        },
        |event| {
            match event {
                install::Event::Retry(r) => writeln!(
                    err,
                    "retry {}/{}: {} ({})",
                    r.attempt, r.max_attempts, r.path, r.error_name
                )?,
                install::Event::Progress(p) => {
                    if p.files_completed <= last
                        || (p.files_completed % 100 != 0 && p.files_completed != p.files_total)
                    {
                        return Ok(());
                    }
                    last = p.files_completed;
                    writeln!(
                        err,
                        "download progress: {}/{} files, {}/{} bytes verified",
                        p.files_completed, p.files_total, p.bytes_verified, p.bytes_total
                    )?;
                }
                install::Event::File(_) => {}
            }
            err.flush()
        },
    )?;
    match result {
        Ok(s) => {
            writeln!(out, "instance\t{}\t{}", s.id, s.name)?;
            writeln!(out, "minecraft\t{}", s.version_id)?;
            if let Some(l) = s.loader {
                writeln!(out, "loader\t{}\t{}", l.name, l.version)?
            } else {
                writeln!(out, "loader\tvanilla")?
            }
            writeln!(
                out,
                "installed\t{}\nskipped\t{}\noverrides\t{}",
                s.files_installed, s.files_skipped, s.overrides_written
            )?;
            Ok(0)
        }
        Err(e) => {
            write_error(err, &e)?;
            Ok(1)
        }
    }
}
fn write_error(err: &mut impl Write, e: &Error) -> io::Result<()> {
    let message = match e.name() {
        "InstanceAlreadyExists" => "an instance with this id already exists",
        "UnknownModpackFormat" => {
            "the file is not a recognized modpack (no modrinth.index.json or manifest.json)"
        }
        "UnsupportedModpackFormat" => "the modpack format version is not supported",
        "UnsupportedModpackLoader" => "the modpack's mod loader is not supported",
        "ModpackLoaderConflict" => "the modpack declares conflicting mod loaders",
        "CurseforgeApiKeyRequired" => {
            "curseforge modpacks require --api-key (supplied by the embedding application)"
        }
        "CurseforgeDownloadUnavailable" => {
            "a project file is not available for third-party downloads"
        }
        "UnsupportedProjectType" => {
            "a project in the pack is not a mod, resource pack, or shader pack"
        }
        "FileNotFound" => "the modpack file does not exist",
        s if s.starts_with("InstancesRootError:") => {
            return writeln!(
                err,
                "failed to open instances directory: {}",
                s.strip_prefix("InstancesRootError:").unwrap()
            );
        }
        s if s.starts_with("StoreRootError:") => {
            return writeln!(
                err,
                "failed to open artifact store: {}",
                s.strip_prefix("StoreRootError:").unwrap()
            );
        }
        _ => return writeln!(err, "modpack installation failed: {}", e.name()),
    };
    writeln!(err, "{message}")
}

fn bad(err: &mut impl Write) -> io::Result<u8> {
    writeln!(err, "error: InvalidArguments")?;
    Ok(1)
}

//! Native content CLI. API-key arguments remain borrowed from caller-owned argv.
use super::{
    Root, curseforge, modrinth,
    registry::{Kind, Provider},
    remote,
};
use crate::{install, instances, store::Store};
use std::io::{self, Write};
fn bad(err: &mut impl Write) -> io::Result<u8> {
    err.write_all(b"error: InvalidArguments\n")?;
    Ok(1)
}
fn number(s: &str) -> Option<u8> {
    let s = s.strip_prefix('+').unwrap_or(s);
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') {
        return None;
    }
    let mut n = 0u8;
    for b in s.bytes() {
        if b == b'_' {
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(b - b'0')?;
    }
    Some(n)
}
#[allow(clippy::too_many_arguments)]
pub fn run_with<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(
    kind: Kind,
    op: &str,
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
    metadata: &M,
    transport: &T,
) -> io::Result<u8> {
    let dp = kind == Kind::Datapack;
    let change = op == "set-version";
    let family = kind.directory();
    let world_usage = if dp { " --world NAME" } else { "" };
    if args.is_empty() {
        if change {
            writeln!(
                err,
                "usage: jmcl-core instance {family} set-version ID{world_usage} --project REF --version ID [--api-key KEY] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]"
            )?
        } else {
            writeln!(
                err,
                "usage: jmcl-core instance {family} install ID{world_usage} --project SLUG [--version ID] [--provider modrinth|curseforge] [--api-key KEY] [--no-dependencies] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]"
            )?
        }
        return Ok(2);
    }
    let id = args[0].as_str();
    if instances::validate_id(id).is_err() {
        return bad(err);
    }
    let (mut project, mut version, mut key, mut world) = (None, None, None, None);
    let mut provider = Provider::Modrinth;
    let (mut root_path, mut store_path) = ("instances", crate::store::DEFAULT_DIRECTORY);
    let (mut workers, mut retries, mut dependencies) = (8, 2, true);
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--no-dependencies" && !change {
            dependencies = false;
            i += 1;
            continue;
        }
        let Some(v) = args
            .get(i + 1)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
        else {
            return bad(err);
        };
        match a {
            "--project" => project = Some(v),
            "--version" => version = Some(v),
            "--api-key" => key = Some(v),
            "--world" if dp => world = Some(v),
            "--instances-directory" => root_path = v,
            "--store-directory" => store_path = v,
            "--workers" => {
                let Some(n) = number(v).filter(|n| (1..=32).contains(n)) else {
                    return bad(err);
                };
                workers = n
            }
            "--retries" => {
                let Some(n) = number(v).filter(|n| *n <= 5) else {
                    return bad(err);
                };
                retries = n
            }
            "--provider" if !change => {
                provider = match v {
                    "modrinth" => Provider::Modrinth,
                    "curseforge" => Provider::Curseforge,
                    _ => return bad(err),
                }
            }
            _ => return bad(err),
        }
        i += 2;
    }
    if dp && world.is_none() {
        writeln!(err, "instance datapacks {op} requires --world")?;
        return Ok(2);
    }
    if project.is_none() || (change && version.is_none()) {
        writeln!(
            err,
            "instance {family} {op} requires --project{}",
            if change { " and --version" } else { "" }
        )?;
        return Ok(2);
    }
    let project = project.unwrap();
    if !change {
        if provider == Provider::Curseforge && key.is_none() {
            err.write_all(
                b"curseforge installs require --api-key (supplied by the embedding application)\n",
            )?;
            return Ok(2);
        }
        if provider == Provider::Modrinth && key.is_some() {
            err.write_all(b"--api-key is only used with --provider curseforge\n")?;
            return Ok(2);
        }
    }
    let instances = match instances::Root::open(root_path) {
        Ok(r) => r,
        Err(e) => {
            writeln!(
                err,
                "failed to open instances directory: {}",
                instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let manifest = match instances.load(id) {
        Ok(m) => m,
        Err(e) => {
            writeln!(
                err,
                "failed to load instance: {}",
                instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let loader = if kind == Kind::Mod {
        if manifest.fabric_loader.is_some() {
            Some("fabric")
        } else if manifest.neoforge_version.is_some() {
            Some("neoforge")
        } else if manifest.forge_version.is_some() {
            Some("forge")
        } else {
            err.write_all(
                b"instance has no mod loader; mods require Fabric, NeoForge, or Forge\n",
            )?;
            return Ok(1);
        }
    } else {
        None
    };
    let game = &manifest.version_id;
    let path = instances::minecraft_path(root_path, id);
    let (root, store) = if dp {
        let store = match Store::open(store_path) {
            Ok(s) => s,
            Err(e) => {
                writeln!(
                    err,
                    "failed to open artifact store: {}",
                    instances::cli_error_name(&e)
                )?;
                return Ok(1);
            }
        };
        let root = match Root::world(&path, world.unwrap(), true) {
            Ok(r) => r,
            Err(e) => {
                failure(err, &super::cli::name(&e), kind, op, project, game, loader)?;
                return Ok(1);
            }
        };
        (root, store)
    } else {
        let root = match Root::open(&path) {
            Ok(r) => r,
            Err(e) => {
                writeln!(
                    err,
                    "failed to open instance directory: {}",
                    super::cli::name(&e)
                )?;
                return Ok(1);
            }
        };
        let store = match Store::open(store_path) {
            Ok(s) => s,
            Err(e) => {
                writeln!(
                    err,
                    "failed to open artifact store: {}",
                    instances::cli_error_name(&e)
                )?;
                return Ok(1);
            }
        };
        (root, store)
    };
    let options = install::Options {
        workers,
        retries,
        sync_files: true,
    };
    let mut completed = 0;
    enum Outcome {
        Installed(remote::ByteSummary),
        Changed(remote::ByteVersionChange),
    }
    let result = install::stream::stream(
        |reporter| {
            if change {
                return root
                    .set_version_bytes(
                        &store,
                        kind,
                        game,
                        loader,
                        project,
                        version.unwrap(),
                        key,
                        metadata,
                        transport,
                        options,
                        Some(reporter),
                    )
                    .map(Outcome::Changed);
            }
            let identifiers = (|| -> modrinth::Result<()> {
                if provider == Provider::Modrinth {
                    modrinth::project_ref(project)?
                } else {
                    curseforge::id(project)?;
                    crate::modpack::provider::validate_key(key.unwrap())
                        .map_err(|e| modrinth::Error(e.name().into()))?
                }
                modrinth::game_version(game)?;
                if let Some(v) = version {
                    if provider == Provider::Modrinth {
                        modrinth::version_id(v)?
                    } else {
                        curseforge::id(v)?
                    }
                }
                Ok(())
            })();
            identifiers.map_err(|e| install::Error::named(&e.0))?;
            let registry = root
                .load_listing()
                .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
            let r = if provider == Provider::Modrinth {
                remote::resolve_modrinth_bytes(
                    metadata,
                    kind,
                    game,
                    loader,
                    project,
                    version,
                    dependencies,
                    &registry,
                )
            } else {
                curseforge::resolve_bytes(
                    metadata,
                    kind,
                    game,
                    loader,
                    key.unwrap(),
                    project,
                    version,
                    dependencies,
                    &registry,
                )
            }
            .map_err(|e| install::Error::named(&e.0))?;
            root.install_resolved_with_registry_bytes(
                &store,
                kind,
                game,
                provider,
                r,
                registry,
                options,
                transport,
                Some(reporter),
            )
            .map(Outcome::Installed)
        },
        |event| {
            match event {
                install::ByteEvent::Retry(r) => {
                    write!(err, "retry {}/{}: ", r.attempt, r.max_attempts)?;
                    err.write_all(&r.path.0)?;
                    writeln!(err, " ({})", r.error_name)?;
                }
                install::ByteEvent::Progress(p)
                    if p.files_completed > completed
                        && (p.files_completed % 100 == 0 || p.files_completed == p.files_total) =>
                {
                    completed = p.files_completed;
                    writeln!(
                        err,
                        "download progress: {}/{} files, {}/{} bytes verified",
                        p.files_completed, p.files_total, p.bytes_verified, p.bytes_total
                    )?
                }
                _ => (),
            }
            err.flush()
        },
    )?;
    match result {
        Err(e) => {
            failure(err, e.name(), kind, op, project, game, loader)?;
            Ok(1)
        }
        Ok(Outcome::Installed(s)) => {
            for f in s.installed {
                out.write_all(b"installed\t")?;
                out.write_all(&f.file.0)?;
                writeln!(
                    out,
                    "\t{}\t{}\t{}",
                    f.sha1,
                    f.slug.as_deref().unwrap_or("-"),
                    if f.as_dependency {
                        "dependency"
                    } else {
                        "explicit"
                    }
                )?;
            }
            writeln!(out, "skipped\t{}", s.skipped)?;
            Ok(0)
        }
        Ok(Outcome::Changed(c)) => {
            if c.changed {
                out.write_all(b"changed\t")?;
                out.write_all(&c.file.0)?;
                writeln!(
                    out,
                    "\t{}\t{}\t{}",
                    c.sha1, c.old_version_id, c.new_version_id
                )?;
            } else {
                out.write_all(b"unchanged\t")?;
                out.write_all(&c.file.0)?;
                writeln!(out, "\t{}", c.new_version_id)?;
            }
            Ok(0)
        }
    }
}
fn failure(
    err: &mut impl Write,
    name: &str,
    kind: Kind,
    op: &str,
    project: &str,
    game: &str,
    loader: Option<&str>,
) -> io::Result<()> {
    let dp = kind == Kind::Datapack;
    let change = op == "set-version";
    let singular = match kind {
        Kind::Mod => "mod",
        Kind::Resourcepack => "resourcepack",
        Kind::Shaderpack => "shaderpack",
        Kind::Datapack => "datapack",
    };
    match name {
        "WorldNotFound" if dp => writeln!(err, "no world matches the given name"),
        "WorldLocked" if dp => writeln!(err, "the world is in use by a running game"),
        "NoCompatibleContentVersion" if dp => writeln!(
            err,
            "no release of the project supports the instance's Minecraft version"
        ),
        "NoCompatibleContentVersion" if !change => {
            if let Some(l) = loader {
                writeln!(err, "no {l} release of {project} supports Minecraft {game}")
            } else {
                writeln!(err, "no release of {project} supports Minecraft {game}")
            }
        }
        "IncompatibleContentVersion" => {
            if dp {
                writeln!(
                    err,
                    "the requested version does not support the instance's Minecraft version"
                )
            } else {
                writeln!(
                    err,
                    "the requested version does not support Minecraft {game}"
                )
            }
        }
        "ContentNotFound" if dp => {
            writeln!(err, "no registered datapack matches the given selector")
        }
        "ContentNotFound" if change => {
            writeln!(err, "no registered {singular} matches the given project")
        }
        "ModrinthVersionProjectMismatch" | "CurseforgeFileProjectMismatch" if change || dp => {
            writeln!(
                err,
                "the requested version does not belong to the registered project"
            )
        }
        "ContentFileConflict" => writeln!(
            err,
            "a different registered content entry already occupies the target file"
        ),
        "ContentKindMismatch" if dp => writeln!(err, "the project is not a datapack project"),
        "ContentKindMismatch" if !change => writeln!(err, "{project} is not a {singular} project"),
        "UnsupportedProjectType" if !change || dp => {
            writeln!(err, "the project type is not supported")
        }
        "CurseforgeApiKeyRequired" => writeln!(
            err,
            "switching a curseforge {}requires --api-key (supplied by the embedding application)",
            if dp { "datapack " } else { "entry " }
        ),
        "CurseforgeDownloadUnavailable" => writeln!(
            err,
            "the project file is not available for third-party downloads"
        ),
        "InvalidCurseforgeId" if dp || !change => {
            writeln!(err, "curseforge projects and files use numeric ids")
        }
        "UnsuccessfulHttpStatus" if dp || !change => writeln!(
            err,
            "the content API returned an unsuccessful HTTP status (check the api key)"
        ),
        _ => {
            if dp {
                writeln!(err, "datapack {op} failed: {name}")
            } else {
                writeln!(
                    err,
                    "content {} failed: {name}",
                    if change {
                        "version change"
                    } else {
                        "installation"
                    }
                )
            }
        }
    }
}

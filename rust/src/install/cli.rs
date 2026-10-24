use super::{
    rpc::{ExecuteParams, PrepareParams, Request},
    service::{Planner, Selection},
    *,
};
use crate::{metadata::Source, version};
use std::io::{self, Write};
fn method(args: &[String]) -> Option<&'static str> {
    match (
        args.first().map(String::as_str),
        args.get(1).map(String::as_str),
    ) {
        (Some("install"), Some("execute")) => Some("execute"),
        (Some("install"), Some("prepare")) => Some("prepare"),
        _ => None,
    }
}
fn usage(method: &str) -> String {
    format!(
        "usage: jmcl-core install {method} ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--directory PATH]{} [--source SOURCE] [--os OS] [--arch ARCH]{}",
        if method == "execute" {
            " [--store-directory PATH]"
        } else {
            ""
        },
        if method == "execute" {
            " [--workers N] [--retries N]"
        } else {
            ""
        }
    )
}
fn number(s: &str) -> Result<u8, ()> {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') {
        return Err(());
    }
    let value = s.replace('_', "").parse::<i16>().map_err(|_| ())?;
    u8::try_from(value).map_err(|_| ())
}
fn parse(method: &str, args: &[String]) -> Result<Request, ()> {
    let id = args.first().filter(|s| !s.is_empty()).ok_or(())?.clone();
    let mut p = ExecuteParams {
        prepare: PrepareParams {
            selection: Selection {
                id,
                fabric_loader: None,
                neoforge_version: None,
                forge_version: None,
                source: Source::Official,
                os: None,
                arch: None,
            },
            directory: ".minecraft".into(),
        },
        store_directory: ".jmclcore".into(),
        workers: 8,
        retries: 2,
        java_paths: vec![],
        java_policy: "auto".into(),
    };
    for pair in args[1..].chunks(2) {
        if pair.len() != 2 {
            return Err(());
        }
        let value = &pair[1];
        match pair[0].as_str() {
            "--directory" if !value.is_empty() => p.prepare.directory = value.clone().into(),
            "--store-directory" if method == "execute" && !value.is_empty() => {
                p.store_directory = value.clone().into()
            }
            "--fabric-loader" if !value.is_empty() => {
                p.prepare.selection.fabric_loader = Some(value.clone())
            }
            "--neoforge-version" if !value.is_empty() => {
                p.prepare.selection.neoforge_version = Some(value.clone())
            }
            "--forge-version" if !value.is_empty() => {
                p.prepare.selection.forge_version = Some(value.clone())
            }
            "--source" => p.prepare.selection.source = Source::parse(value).ok_or(())?,
            "--os" => p.prepare.selection.os = Some(value.clone()),
            "--arch" => {
                p.prepare.selection.arch = Some(
                    match value.as_str() {
                        "amd64" => "x86_64",
                        "aarch64" => "arm64",
                        _ => value,
                    }
                    .into(),
                )
            }
            "--workers" if method == "execute" => {
                p.workers = number(value)?;
                if !(1..=32).contains(&p.workers) {
                    return Err(());
                }
            }
            "--retries" if method == "execute" => {
                p.retries = number(value)?;
                if p.retries > 5 {
                    return Err(());
                }
            }
            "--java-policy"
                if method == "execute"
                    && matches!(value.as_str(), "auto" | "local" | "managed") =>
            {
                p.java_policy = value.clone()
            }
            _ => return Err(()),
        }
    }
    if !p.prepare.selection.valid_target() {
        return Err(());
    }
    if method == "execute" {
        Ok(Request::Execute(p))
    } else {
        // Pinned CLI prepare parses this option but omits it from selection.
        // Keep this observable behavior until an explicitly approved defect fix.
        p.prepare.selection.forge_version = None;
        Ok(Request::Prepare(p.prepare))
    }
}
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Option<io::Result<u8>> {
    let method = method(args)?;
    let request = match parse(method, &args[2..]) {
        Ok(p) => p,
        Err(()) => return Some(writeln!(err, "{}", usage(method)).map(|_| 2)),
    };
    let metadata = match version::HttpTransport::new() {
        Ok(v) => v,
        Err(e) => {
            return Some(
                writeln!(
                    err,
                    "failed to create install plan from Mojang: {}",
                    e.name()
                )
                .map(|_| 1),
            );
        }
    };
    let transport = match HttpTransport::new() {
        Ok(v) => v,
        Err(e) => return Some(writeln!(err, "download failed: {}", e.name()).map(|_| 1)),
    };
    Some(run(
        request,
        &version::Service::new(metadata, ".jmclcore/metadata-cache"),
        &transport,
        out,
        err,
    ))
}
pub fn cli_with<P: Planner, T: Transport>(
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
    planner: &P,
    transport: &T,
) -> Option<io::Result<u8>> {
    let method = method(args)?;
    Some(match parse(method, &args[2..]) {
        Ok(p) => run(p, planner, transport, out, err),
        Err(()) => writeln!(err, "{}", usage(method)).map(|_| 2),
    })
}
fn notice(out: &mut impl Write, source: Source) -> io::Result<()> {
    if source == Source::Bmclapi {
        writeln!(out, "# source\t{}", source.display())?;
    }
    Ok(())
}
pub(crate) fn run<P: Planner, T: Transport>(
    request: Request,
    planner: &P,
    transport: &T,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    run_with_environment(
        request,
        planner,
        transport,
        out,
        err,
        &std::env::vars_os().collect(),
    )
}
fn run_with_environment<P: Planner, T: Transport>(
    request: Request,
    planner: &P,
    transport: &T,
    out: &mut impl Write,
    err: &mut impl Write,
    environment: &crate::java::detect::Environment,
) -> io::Result<u8> {
    let base = match &request {
        Request::Execute(p) => &p.prepare,
        Request::Prepare(p) => p,
    };
    let selection = &base.selection;
    let target = match selection.target() {
        Ok(t) => t,
        Err(e) => {
            writeln!(err, "unsupported host platform: {}", e.name())?;
            return Ok(1);
        }
    };
    let plan = match planner.plan(selection, target.clone()) {
        Ok(p) => p,
        Err(e) => {
            writeln!(
                err,
                "failed to create install plan from {}: {}",
                selection.source.display(),
                e.name()
            )?;
            return Ok(1);
        }
    };
    if matches!(&request, Request::Execute(_))
        && matches!(plan.loader.as_str(), "forge" | "neoforge")
        && service::processor_loader(&plan, selection).is_none()
    {
        writeln!(err, "processor chain failed: MissingLoaderVersion")?;
        return Ok(1);
    }
    let root = match base.directory.path().and_then(ArtifactRoot::open) {
        Ok(r) => r,
        Err(e) => {
            writeln!(
                err,
                "failed to open install root: {}",
                Error::from(e).name()
            )?;
            return Ok(1);
        }
    };
    match request {
        Request::Execute(p) => {
            let store = match p.store_directory.path().and_then(Store::open) {
                Ok(s) => s,
                Err(e) => {
                    writeln!(
                        err,
                        "failed to open artifact store: {}",
                        Error::from(e).name()
                    )?;
                    return Ok(1);
                }
            };
            let mut last_completed = 0;
            let result = stream::stream(
                |sink| {
                    execute(
                        &root,
                        &store,
                        &plan,
                        Options {
                            workers: p.workers,
                            retries: p.retries,
                            sync_files: true,
                        },
                        transport,
                        Some(sink),
                    )
                },
                |event| {
                    match event {
                        Event::Retry(r) => writeln!(
                            err,
                            "retry {}/{}: {} ({})",
                            r.attempt, r.max_attempts, r.path, r.error_name
                        )?,
                        Event::Progress(v)
                            if v.files_completed > last_completed
                                && (v.files_completed % 100 == 0
                                    || v.files_completed == v.files_total) =>
                        {
                            last_completed = v.files_completed;
                            writeln!(
                                err,
                                "download progress: {}/{} files, {}/{} bytes verified",
                                v.files_completed, v.files_total, v.bytes_verified, v.bytes_total
                            )?;
                        }
                        _ => {}
                    }
                    err.flush()
                },
            )?;
            let summary = match result {
                Ok(s) => s,
                Err(e) => {
                    writeln!(err, "download failed: {}", e.name())?;
                    return Ok(1);
                }
            };
            if let Some(loader) = service::processor_loader(&plan, &p.prepare.selection) {
                let java = match service::processor_java(
                    &plan,
                    &p.java_paths,
                    &p.java_policy,
                    &p.store_directory.path()?,
                    environment,
                ) {
                    Ok(java) => java,
                    Err(error) => {
                        writeln!(err, "processor Java resolution failed: {error}")?;
                        return Ok(1);
                    }
                };
                let options = crate::loaders::Options {
                    java_paths: vec![java],
                    required_major: plan.java_major_version,
                    ..Default::default()
                };
                let result = stream::stream(
                    |sink| {
                        crate::loaders::run(
                            &root,
                            &store,
                            loader,
                            &plan.version_id,
                            &options,
                            Some(environment),
                            &mut |event| sink(event),
                        )
                    },
                    |event| {
                        writeln!(
                            err,
                            "processor {}/{}: {}",
                            event.index, event.total, event.name
                        )?;
                        err.flush()
                    },
                )?;
                let patched = match result {
                    Ok(summary) => summary,
                    Err(error) => {
                        let name = if plan.loader == "forge" {
                            "Forge"
                        } else {
                            "NeoForge"
                        };
                        writeln!(err, "{name} processor chain failed: {}", error.name())?;
                        return Ok(1);
                    }
                };
                writeln!(
                    out,
                    "processors\t{}\npatchedClient\t{}",
                    patched.processors_run, patched.patched_sha1
                )?;
            }
            if let Err(e) = service::record_completed(&root, &p.prepare.selection, &target) {
                writeln!(err, "failed to record completed installation: {}", e.name())?;
                return Ok(1);
            }
            notice(out, p.prepare.selection.source)?;
            writeln!(
                out,
                "version\t{}\ndownloaded\t{}\ncached\t{}\nhardLinked\t{}\ncopied\t{}\nverifiedBytes\t{}\ntransferredBytes\t{}",
                p.prepare.selection.id,
                summary.files_downloaded,
                summary.files_cached,
                summary.files_hard_linked,
                summary.files_copied,
                summary.bytes_verified,
                summary.bytes_transferred
            )?;
        }
        Request::Prepare(p) => {
            let prepared = match prepare(&root, &plan) {
                Ok(r) => r,
                Err(e) => {
                    writeln!(err, "runtime preparation failed: {}", e.name())?;
                    return Ok(1);
                }
            };
            notice(out, p.selection.source)?;
            writeln!(out, "version\t{}", p.selection.id)?;
            if let Some(path) = prepared.native_directory {
                writeln!(out, "nativeDirectory\t{path}")?;
            }
            writeln!(
                out,
                "nativeFiles\t{}\nprojectionsCreated\t{}\nprojectionsCached\t{}",
                prepared.native_files, prepared.projections_created, prepared.projections_cached
            )?;
        }
    }
    Ok(0)
}

/// Explicit process environment for embedding and offline fixture execution.
pub fn cli_with_environment<P: Planner, T: Transport>(
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
    planner: &P,
    transport: &T,
    environment: &crate::java::detect::Environment,
) -> Option<io::Result<u8>> {
    let method = method(args)?;
    Some(match parse(method, &args[2..]) {
        Ok(request) => run_with_environment(request, planner, transport, out, err, environment),
        Err(()) => writeln!(err, "{}", usage(method)).map(|_| 2),
    })
}

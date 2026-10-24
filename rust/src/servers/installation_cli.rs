use std::io::{self, Write};
const USAGE: &str = "usage: jmcl-core server install ID --accept-eula [--servers-directory PATH] [--store-directory PATH] [--workers N] [--retries N] [--java-path PATH ...] [--json]";
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    if !cfg!(target_os = "linux") {
        writeln!(
            err,
            "Server management is currently supported only on Linux"
        )?;
        return Ok(1);
    }
    #[cfg(target_os = "linux")]
    {
        let Some((p, json_output)) = parse(args) else {
            writeln!(err, "{USAGE}")?;
            return Ok(2);
        };
        if !p.accept_eula {
            writeln!(
                err,
                "Server installation requires explicit Minecraft EULA acceptance"
            )?;
            return Ok(1);
        }
        let metadata = match crate::version::HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(
                    err,
                    "failed to create the server installation plan: {}",
                    e.name()
                )?;
                return Ok(1);
            }
        };
        let planner = crate::version::Service::new(metadata, ".jmclcore/metadata-cache");
        let transport = match crate::install::HttpTransport::new() {
            Ok(t) => t,
            Err(_) => {
                writeln!(
                    err,
                    "failed to download and materialize the server jar: ServerInstallFailed"
                )?;
                return Ok(1);
            }
        };
        run(
            p,
            json_output,
            &planner,
            &transport,
            Some(&std::env::vars_os().collect()),
            out,
            err,
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (args, out, err, USAGE);
        unreachable!()
    }
}
#[cfg(target_os = "linux")]
fn parse(args: &[String]) -> Option<(super::installation::linux::Params, bool)> {
    let id = args.first()?;
    crate::instances::validate_id(id).ok()?;
    let mut p = super::installation::linux::Params {
        directory: "servers".into(),
        id: id.clone(),
        accept_eula: false,
        store_directory: ".jmclcore".into(),
        workers: 8,
        retries: 2,
        java_paths: vec![],
    };
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--accept-eula" => {
                p.accept_eula = true;
                i += 1;
                continue;
            }
            "--json" => {
                json = true;
                i += 1;
                continue;
            }
            _ => {}
        }
        let v = args.get(i + 1)?;
        if v.is_empty() {
            return None;
        }
        match args[i].as_str() {
            "--servers-directory" => p.directory = v.clone(),
            "--store-directory" => p.store_directory = v.clone(),
            "--java-path" => p.java_paths.push(v.clone()),
            "--workers" => {
                p.workers = v.parse().ok()?;
                if p.workers == 0 || p.workers > 32 {
                    return None;
                }
            }
            "--retries" => {
                p.retries = v.parse().ok()?;
                if p.retries > 5 {
                    return None;
                }
            }
            _ => return None,
        }
        i += 2;
    }
    Some((p, json))
}
#[cfg(target_os = "linux")]
fn run(
    p: super::installation::linux::Params,
    json_output: bool,
    planner: &impl super::planning::Planner,
    transport: &impl crate::install::Transport,
    env: Option<&crate::java::detect::Environment>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    use super::installation::linux;
    use crate::install::{self, Event};
    let mut detail = None;
    let p = match linux::prepare_detailed(p, planner, env, &mut detail) {
        Ok(p) => p,
        Err(e) => {
            writeln!(err, "{}", detail.unwrap_or_else(|| lower(&e.message)))?;
            return Ok(1);
        }
    };
    let mut last = 0;
    let result = install::stream::stream(
        |sink| linux::execute(&p, transport, env, sink),
        |event| {
            match event {
                Event::Retry(r) => writeln!(
                    err,
                    "retry {}/{}: {} ({})",
                    r.attempt, r.max_attempts, r.path, r.error_name
                )?,
                Event::Progress(v)
                    if v.files_completed > last
                        && (v.files_completed % 100 == 0 || v.files_completed == v.files_total) =>
                {
                    last = v.files_completed;
                    writeln!(
                        err,
                        "download progress: {}/{} files, {}/{} bytes verified",
                        v.files_completed, v.files_total, v.bytes_verified, v.bytes_total
                    )?
                }
                _ => {}
            }
            err.flush()
        },
    )?;
    let result = match result {
        Ok(v) => v,
        Err(e) => {
            if e.code == "SERVER_INSTALL_FAILED" {
                writeln!(
                    err,
                    "failed to download and materialize the server jar: ServerInstallFailed"
                )?
            } else {
                writeln!(err, "{}", lower(&e.message))?
            }
            return Ok(1);
        }
    };
    if json_output {
        serde_json::to_writer(&mut *out, &result)?;
        writeln!(out)?
    } else {
        if p.plan.source == crate::metadata::Source::Bmclapi {
            writeln!(out, "# source\t{}", p.plan.source.display())?
        }
        writeln!(
            out,
            "server\t{}\nversion\t{}\nsource\t{}\njava\t{}\npropertiesCreated\t{}",
            p.manifest.id,
            p.manifest.version_id,
            p.manifest.source,
            p.runtime.executable,
            result["properties_created"]
        )?;
        for (key, label) in [
            ("files_total", "files"),
            ("files_downloaded", "downloaded"),
            ("files_cached", "cached"),
            ("files_hard_linked", "hardLinked"),
            ("files_copied", "copied"),
            ("bytes_verified", "verifiedBytes"),
            ("bytes_transferred", "transferredBytes"),
        ] {
            writeln!(out, "{label}\t{}", result[key])?
        }
    }
    Ok(0)
}
#[cfg(target_os = "linux")]
fn lower(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

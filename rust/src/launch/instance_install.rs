//! Instance installation delegates to the same verified installer and receipt
//! writer as global installation; it does not invent a second execution path.
use crate::{
    install::{
        self,
        rpc::{ExecuteParams, PrepareParams, Request},
        service::{Planner, Selection},
    },
    instances,
    metadata::Source,
    version,
};
use std::io::{self, Write};
const USAGE: &str = "usage: jmcl-core instance install ID [--instances-directory PATH] [--store-directory PATH] [--source SOURCE] [--os OS] [--arch ARCH] [--workers N] [--retries N] [--java PATH]";
struct Options<'a> {
    id: &'a str,
    root: &'a str,
    store: &'a str,
    source: Option<Source>,
    os: Option<String>,
    arch: Option<String>,
    workers: u8,
    retries: u8,
    java: Vec<String>,
    policy: String,
}
fn number(s: &str) -> Result<u8, ()> {
    let negative = s.starts_with('-');
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') {
        return Err(());
    }
    let mut n = 0u8;
    for c in digits.bytes() {
        if c == b'_' {
            continue;
        }
        if !c.is_ascii_digit() {
            return Err(());
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add(c - b'0'))
            .ok_or(())?;
    }
    if negative && n != 0 { Err(()) } else { Ok(n) }
}
fn parse(args: &[String]) -> Result<Options<'_>, ()> {
    let id = args.first().ok_or(())?;
    instances::validate_id(id).map_err(|_| ())?;
    let mut p = Options {
        id,
        root: "instances",
        store: ".jmclcore",
        source: None,
        os: None,
        arch: None,
        workers: 8,
        retries: 2,
        java: vec![],
        policy: "auto".into(),
    };
    for pair in args[1..].chunks(2) {
        if pair.len() != 2 || pair[1].is_empty() {
            return Err(());
        }
        let v = pair[1].as_str();
        match pair[0].as_str() {
            "--instances-directory" => p.root = v,
            "--store-directory" => p.store = v,
            "--source" => p.source = Some(Source::parse(v).ok_or(())?),
            "--os" => {
                if !matches!(v, "linux" | "windows" | "osx") {
                    return Err(());
                }
                p.os = Some(v.into())
            }
            "--arch" => {
                let v = match v {
                    "amd64" => "x86_64",
                    "aarch64" => "arm64",
                    _ => v,
                };
                if !matches!(v, "x86" | "x86_64" | "arm" | "arm64") {
                    return Err(());
                }
                p.arch = Some(v.into())
            }
            "--workers" => {
                p.workers = number(v)?;
                if !(1..=32).contains(&p.workers) {
                    return Err(());
                }
            }
            "--retries" => {
                p.retries = number(v)?;
                if p.retries > 5 {
                    return Err(());
                }
            }
            "--java" => p.java.push(v.into()),
            "--java-policy" => {
                if !matches!(v, "auto" | "local" | "managed") {
                    return Err(());
                }
                p.policy = v.into()
            }
            _ => return Err(()),
        }
    }
    Ok(p)
}
fn request_with<'a, T, R>(
    args: &'a [String],
    err: &mut impl Write,
    load: fn(&instances::Root, &str) -> io::Result<T>,
    convert: impl FnOnce(Options<'a>, T) -> R,
) -> io::Result<Result<R, u8>> {
    let p = match parse(args) {
        Ok(p) => p,
        Err(()) => {
            writeln!(err, "{USAGE}")?;
            return Ok(Err(2));
        }
    };
    let root = match instances::Root::open(p.root) {
        Ok(r) => r,
        Err(e) => {
            writeln!(
                err,
                "failed to open instances directory: {}",
                instances::cli_error_name(&e)
            )?;
            return Ok(Err(1));
        }
    };
    let m = match load(&root, p.id) {
        Ok(m) => m,
        Err(e) => {
            writeln!(
                err,
                "failed to load instance: {}",
                instances::cli_error_name(&e)
            )?;
            return Ok(Err(1));
        }
    };
    Ok(Ok(convert(p, m)))
}
fn legacy(p: Options<'_>, m: instances::Manifest) -> Request {
    Request::Execute(ExecuteParams {
        prepare: PrepareParams {
            selection: Selection {
                id: m.version_id,
                fabric_loader: m.fabric_loader,
                neoforge_version: m.neoforge_version,
                forge_version: m.forge_version,
                source: p.source.unwrap_or_else(|| {
                    Source::parse(&m.source).expect("validated instance source")
                }),
                os: p.os,
                arch: p.arch,
            },
            directory: instances::minecraft_path(p.root, p.id).into(),
        },
        store_directory: p.store.into(),
        workers: p.workers,
        retries: p.retries,
        java_paths: p.java,
        java_policy: p.policy,
    })
}
fn request(args: &[String], err: &mut impl Write) -> io::Result<Result<Request, u8>> {
    request_with(args, err, instances::Root::load, legacy)
}
enum ManagedRequest {
    Vanilla(install::vanilla::Request),
    Legacy(Request),
}
fn managed_request(
    args: &[String],
    err: &mut impl Write,
) -> io::Result<Result<ManagedRequest, u8>> {
    request_with(
        args,
        err,
        instances::Root::load_install_selection,
        |p, m| match m {
            instances::InstallSelection::Legacy(m) => ManagedRequest::Legacy(legacy(p, m)),
            instances::InstallSelection::Vanilla { version, source } => {
                ManagedRequest::Vanilla(install::vanilla::Request {
                    id: version,
                    source: p.source.unwrap_or(source),
                    directory: instances::minecraft_path(p.root, p.id).into_bytes(),
                    store: p.store.as_bytes().to_vec(),
                    os: p.os,
                    arch: p.arch,
                    workers: p.workers,
                    retries: p.retries,
                })
            }
        },
    )
}
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Option<io::Result<u8>> {
    if args.first().map(String::as_str) != Some("instance")
        || args.get(1).map(String::as_str) != Some("install")
    {
        return None;
    }
    Some((|| {
        let req = match managed_request(&args[2..], err)? {
            Ok(r) => r,
            Err(c) => return Ok(c),
        };
        let metadata = match version::HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(
                    err,
                    "failed to create install plan from Mojang: {}",
                    e.name()
                )?;
                return Ok(1);
            }
        };
        let transport = match install::HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(err, "download failed: {}", e.name())?;
                return Ok(1);
            }
        };
        let service = version::Service::new(metadata, ".jmclcore/metadata-cache");
        match req {
            ManagedRequest::Vanilla(p) => {
                install::vanilla::run_cli(p, &service, &transport, out, err)
            }
            ManagedRequest::Legacy(p) => install::cli::run(p, &service, &transport, out, err),
        }
    })())
}
/// Fully injected fixtures verify hardlinks and receipts without fetching game artifacts.
pub fn run_with<P: Planner, T: install::Transport>(
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
    planner: &P,
    transport: &T,
) -> io::Result<u8> {
    match request(args, err)? {
        Ok(r) => install::cli::run(r, planner, transport, out, err),
        Err(c) => Ok(c),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;

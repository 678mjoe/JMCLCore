//! Literal offline launch CLI. No shell, credential persistence or runtime archive acquisition.
use super::{
    Auth, Options, executor,
    process::{Event, RecordEnd, Stream},
};
use crate::{instances, java::managed, metadata::Source, version};
use std::io::{self, Write};
const USAGE: &str = "usage: jmcl-core launch execute ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--directory PATH] --offline-name NAME [--source SOURCE] [--arch ARCH] [--java PATH]";
const INSTANCE_USAGE: &str = "usage: jmcl-core instance launch ID --offline-name NAME [--instances-directory PATH] [--arch ARCH] [--java PATH]";
struct Args<'a> {
    id: &'a str,
    directory: &'a str,
    instances: &'a str,
    name: &'a str,
    source: Source,
    arch: Option<&'a str>,
    fabric: Option<&'a str>,
    neo: Option<&'a str>,
    forge: Option<&'a str>,
    java: Vec<&'a str>,
    policy: managed::Policy,
    override_: Option<&'a str>,
    store: &'a str,
}
fn parse(args: &[String], instance: bool) -> Result<Args<'_>, ()> {
    let id = args.first().filter(|s| !s.is_empty()).ok_or(())?;
    if instance {
        instances::validate_id(id).map_err(|_| ())?
    }
    let mut p = Args {
        id,
        directory: ".minecraft",
        instances: "instances",
        name: "",
        source: Source::Official,
        arch: None,
        fabric: None,
        neo: None,
        forge: None,
        java: vec![],
        policy: managed::Policy::Auto,
        override_: None,
        store: ".jmclcore",
    };
    let mut i = 1;
    while i < args.len() {
        let v = args
            .get(i + 1)
            .filter(|v| !v.is_empty())
            .ok_or(())?
            .as_str();
        match args[i].as_str() {
            "--directory" if !instance => p.directory = v,
            "--instances-directory" if instance => p.instances = v,
            "--fabric-loader" if !instance => p.fabric = Some(v),
            "--neoforge-version" if !instance => p.neo = Some(v),
            "--forge-version" if !instance => p.forge = Some(v),
            "--offline-name" => p.name = v,
            "--source" if !instance => p.source = Source::parse(v).ok_or(())?,
            "--arch" => {
                let v = match v {
                    "amd64" => "x86_64",
                    "aarch64" => "arm64",
                    _ => v,
                };
                if !matches!(v, "x86" | "x86_64" | "arm" | "arm64") {
                    return Err(());
                }
                p.arch = Some(v)
            }
            "--java" => p.java.push(v),
            "--java-policy" => {
                p.policy = match v {
                    "auto" => managed::Policy::Auto,
                    "local" => managed::Policy::Local,
                    "managed" => managed::Policy::Managed,
                    _ => return Err(()),
                }
            }
            "--java-override" => p.override_ = Some(v),
            "--store" if !instance => p.store = v,
            "--store-directory" if instance => p.store = v,
            _ => return Err(()),
        };
        i += 2
    }
    if p.name.is_empty() || (p.override_.is_some() && !p.java.is_empty()) {
        return Err(());
    }
    Ok(p)
}
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Option<io::Result<u8>> {
    if let Some(result) = super::instance_install::run(args, out, err) {
        return Some(result);
    }
    let instance =
        args.first().is_some_and(|s| s == "instance") && args.get(1).is_some_and(|s| s == "launch");
    if !instance && !args.first().is_some_and(|s| s == "launch") {
        return None;
    }
    if !instance && !args.get(1).is_some_and(|s| s == "execute") {
        return Some((|| {
            writeln!(
                err,
                "usage: jmcl-core launch execute ID [--directory PATH] --offline-name NAME [options]"
            )?;
            Ok(2)
        })());
    }
    Some((|| {
        let mut p = match parse(&args[2..], instance) {
            Ok(p) => p,
            Err(()) => {
                writeln!(err, "{}", if instance { INSTANCE_USAGE } else { USAGE })?;
                return Ok(2);
            }
        };
        let manifest;
        let path;
        if instance {
            let root = match instances::Root::open(p.instances) {
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
            manifest = match root.load(p.id) {
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
            path = instances::minecraft_path(p.instances, p.id);
            p.id = &manifest.version_id;
            p.fabric = manifest.fabric_loader.as_deref();
            p.neo = manifest.neoforge_version.as_deref();
            p.forge = manifest.forge_version.as_deref();
            p.source = Source::parse(&manifest.source).expect("validated instance source");
            p.directory = &path;
        }
        execute(p, out, err)
    })())
}
fn execute(p: Args<'_>, out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    let uuid = super::offline_uuid::uuid(p.name);
    let transport = match version::HttpTransport::new() {
        Ok(t) => t,
        Err(_) => {
            writeln!(err, "game launch failed: MetadataFailed")?;
            return Ok(1);
        }
    };
    let service = version::Service::new(transport, ".jmclcore/metadata-cache");
    let env = std::env::vars_os().collect();
    let prepared = executor::prepare(
        &service,
        executor::Input {
            id: p.id,
            fabric_loader: p.fabric,
            neoforge_version: p.neo,
            forge_version: p.forge,
            directory: p.directory,
            auth: Auth {
                player_name: p.name,
                uuid: &uuid,
                access_token: "offline",
                session: None,
                user_type: "legacy",
                xuid: "",
                client_id: "",
                user_properties: "{}",
            },
            options: Options::default(),
            source: p.source,
            arch: p.arch,
            java_paths: &p.java,
            java_policy: p.policy,
            java_override: p.override_,
            store_directory: p.store,
        },
        Some(&env),
        |request| managed::resolve(Some(&env), request, &mut |_, _| Err("JavaDownloadFailed")),
    );
    let prepared = match prepared {
        Ok(p) => p,
        Err(e) => {
            writeln!(err, "game launch failed: {e}")?;
            return Ok(1);
        }
    };
    let mut failed = false;
    let result = prepared.run(&env, |event, _| {
        if failed {
            return;
        }
        let result = match event {
            Event::Started { pid } => {
                writeln!(err, "game started: pid {pid}").and_then(|()| err.flush())
            }
            Event::Log { stream, bytes, end } => {
                let writer: &mut dyn Write = match stream {
                    Stream::Stdout => out,
                    Stream::Stderr => err,
                };
                writer
                    .write_all(bytes)
                    .and_then(|()| {
                        if end == RecordEnd::Newline {
                            writer.write_all(b"\n")
                        } else {
                            Ok(())
                        }
                    })
                    .and_then(|()| writer.flush())
            }
        };
        failed = result.is_err();
    });
    if failed {
        return Err(io::Error::other("WriteFailed"));
    }
    let r = match result {
        Ok(r) => r,
        Err(e) => {
            writeln!(err, "game launch failed: {e}")?;
            return Ok(1);
        }
    };
    write!(err, "game stopped: {}", r.process.termination)?;
    if let Some(code) = r.process.exit_code {
        write!(err, " ({code})")?
    }
    writeln!(err, ", native cleanup: {}", r.native_cleanup)?;
    Ok(if r.process.termination == "exited" {
        r.process.exit_code.unwrap()
    } else {
        1
    })
}

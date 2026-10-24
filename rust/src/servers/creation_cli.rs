use std::io::{self, Write};
const USAGE: &str = "usage: jmcl-core server create ID --version VERSION [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--name NAME] [--source SOURCE] [--java-path PATH] [--servers-directory PATH] [--json]";
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
        let transport = match crate::version::HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                writeln!(err, "failed to resolve Minecraft version: {}", e.name())?;
                return Ok(1);
            }
        };
        let service = crate::version::Service::new(transport, ".jmclcore/metadata-cache");
        run(p, json_output, &service, out, err)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (args, out, err, USAGE);
        unreachable!()
    }
}
#[cfg(target_os = "linux")]
fn parse(args: &[String]) -> Option<(super::creation::linux::Params, bool)> {
    use serde_json::json;
    let id = args.first()?;
    crate::instances::validate_id(id).ok()?;
    let mut p = json!({"directory":"servers","id":id});
    let mut output = false;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--json" {
            output = true;
            i += 1;
            continue;
        }
        let value = args.get(i + 1)?;
        if value.is_empty() {
            return None;
        }
        let key = match args[i].as_str() {
            "--version" => "version_id",
            "--fabric-loader" => "fabric_loader",
            "--neoforge-version" => "neoforge_version",
            "--forge-version" => "forge_version",
            "--name" => "name",
            "--source" => "source",
            "--java-path" => "java_path",
            "--servers-directory" => "directory",
            _ => return None,
        };
        p[key] = value.clone().into();
        i += 2;
    }
    let raw = serde_json::value::to_raw_value(&p).ok()?;
    super::creation::linux::parse(Some(&raw))
        .ok()
        .map(|p| (p, output))
}
#[cfg(target_os = "linux")]
pub(super) fn io_name(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::AlreadyExists => "PathAlreadyExists",
        io::ErrorKind::NotADirectory => "NotDir",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        io::ErrorKind::IsADirectory => "IsDir",
        io::ErrorKind::InvalidData => return e.to_string(),
        _ => "Unexpected",
    }
    .into()
}
#[cfg(target_os = "linux")]
fn run(
    p: super::creation::linux::Params,
    json_output: bool,
    planner: &impl super::planning::Planner,
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    use super::{manifest::Manifest, store};
    let m = Manifest {
        schema_version: 1,
        id: p.id.clone(),
        name: p.name.unwrap_or(p.id),
        version_id: p.version_id,
        fabric_loader: p.fabric_loader,
        neoforge_version: p.neoforge_version,
        forge_version: p.forge_version,
        source: p.source,
        java_path: p.java_path,
        accept_eula: false,
    };
    if let Err(e) = planner.resolve(&m) {
        writeln!(err, "failed to resolve Minecraft version: {}", e.name())?;
        return Ok(1);
    }
    let root = match store::open_root(&p.directory) {
        Ok(r) => r,
        Err(e) => {
            writeln!(err, "failed to open servers directory: {}", io_name(&e))?;
            return Ok(1);
        }
    };
    let result = (|| -> Result<(), String> {
        m.validate().map_err(str::to_owned)?;
        rustix::fs::mkdirat(&root, m.id.as_str(), rustix::fs::Mode::from_raw_mode(0o755))
            .map_err(|e| io_name(&io::Error::from(e)))?;
        if let Err(e) =
            store::open_server(&root, &m.id).and_then(|dir| store::write_manifest(&dir, &m))
        {
            let _ = rustix::fs::unlinkat(&root, m.id.as_str(), rustix::fs::AtFlags::REMOVEDIR);
            return Err(io_name(&e));
        }
        Ok(())
    })();
    if let Err(e) = result {
        writeln!(err, "failed to create server: {e}")?;
        return Ok(1);
    }
    if json_output {
        let mut v = serde_json::to_value(&m)?;
        v.as_object_mut().unwrap().remove("schema_version");
        v["installed"] = false.into();
        serde_json::to_writer(&mut *out, &serde_json::json!({"server":v}))?;
        writeln!(out)?;
    } else {
        writeln!(
            out,
            "server\t{}\nname\t{}\nversion\t{}\nsource\t{}\ninstalled\tfalse",
            m.id, m.name, m.version_id, m.source
        )?
    }
    Ok(0)
}

#[cfg(any(unix, windows))]
use super::Root;
use super::{Manifest, validate_id};
use serde_json::json;
use std::io::{self, Write};
const CREATE_USAGE: &str = "usage: jmcl-core instance create ID --version VERSION [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--name NAME] [--instances-directory PATH] [--source SOURCE] [--no-validate]\n";
const LIST_USAGE: &str = "usage: jmcl-core instance list [--instances-directory PATH] [--json]\n";
const SHOW_USAGE: &str =
    "usage: jmcl-core instance show ID [--instances-directory PATH] [--json]\n";
const DELETE_USAGE: &str =
    "usage: jmcl-core instance delete ID --force [--instances-directory PATH]\n";
struct Options {
    directory: String,
    id: Option<String>,
    json: bool,
    force: bool,
    validate: bool,
    name: Option<String>,
    version: Option<String>,
    fabric: Option<String>,
    neoforge: Option<String>,
    forge: Option<String>,
    source: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            directory: "instances".into(),
            id: None,
            json: false,
            force: false,
            validate: true,
            name: None,
            version: None,
            fabric: None,
            neoforge: None,
            forge: None,
            source: "official".into(),
        }
    }
}
fn options(command: &str, args: &[String]) -> Option<Options> {
    let mut o = Options::default();
    let mut i = 0;
    if command != "list" {
        let id = args.first()?;
        validate_id(id).ok()?;
        o.id = Some(id.clone());
        i = 1;
    }
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--json" if matches!(command, "list" | "show") => o.json = true,
            "--force" if command == "delete" => o.force = true,
            "--no-validate" if command == "create" => o.validate = false,
            _ => {
                let value = args.get(i + 1)?.clone();
                if value.is_empty() {
                    return None;
                }
                match arg {
                    "--instances-directory" => o.directory = value,
                    "--version" if command == "create" => o.version = Some(value),
                    "--fabric-loader" if command == "create" => o.fabric = Some(value),
                    "--neoforge-version" if command == "create" => o.neoforge = Some(value),
                    "--forge-version" if command == "create" => o.forge = Some(value),
                    "--name" if command == "create" => o.name = Some(value),
                    "--source" if command == "create" => {
                        if !matches!(value.as_str(), "official" | "bmclapi") {
                            return None;
                        }
                        o.source = value
                    }
                    _ => return None,
                }
                i += 1;
            }
        }
        i += 1;
    }
    if command == "create" && o.version.is_none() {
        return None;
    }
    Some(o)
}
pub(crate) fn zig_error(e: &io::Error) -> String {
    // Pinned Zig native create/open maps EILSEQ to BadPathName. This shared
    // boundary is also used for modpack override writes on APFS.
    #[cfg(unix)]
    if e.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error()) {
        return "BadPathName".into();
    }
    // Internal validation returns the exact Zig error name; OS errors map at boundary.
    if e.kind() == io::ErrorKind::InvalidData {
        return e.to_string();
    }
    match e.raw_os_error() {
        Some(2) => "FileNotFound",
        Some(13 | 1) => "AccessDenied",
        Some(17) => "PathAlreadyExists",
        Some(20) => "NotDir",
        Some(21) => "IsDir",
        Some(40) => "SymLinkLoop",
        Some(36) => "NameTooLong",
        Some(28) => "NoSpaceLeft",
        Some(30) => "ReadOnlyFileSystem",
        _ => match e.kind() {
            io::ErrorKind::NotFound => "FileNotFound",
            io::ErrorKind::AlreadyExists => "PathAlreadyExists",
            io::ErrorKind::PermissionDenied => "AccessDenied",
            _ => "Unexpected",
        },
    }
    .into()
}
fn usage(out: &mut impl Write, text: &str) -> io::Result<u8> {
    out.write_all(text.as_bytes())?;
    Ok(2)
}
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Option<io::Result<u8>> {
    let command = args.first()?.as_str();
    if !matches!(command, "create" | "list" | "show" | "delete") {
        return None;
    }
    Some(run_inner(command, &args[1..], out, err))
}
fn run_inner(
    command: &str,
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let Some(o) = options(command, args) else {
        return usage(
            err,
            match command {
                "create" => CREATE_USAGE,
                "list" => LIST_USAGE,
                "show" => SHOW_USAGE,
                _ => DELETE_USAGE,
            },
        );
    };
    if command == "delete" && !o.force {
        return usage(err, "instance deletion requires --force\n");
    }
    if command == "create" {
        if [&o.fabric, &o.neoforge, &o.forge]
            .iter()
            .filter(|v| v.is_some())
            .count()
            > 1
        {
            return usage(
                err,
                "only one of --fabric-loader, --neoforge-version, or --forge-version may be given\n",
            );
        }
        if o.validate
            && let Err(e) = super::validate_selection(
                o.version.as_deref().unwrap(),
                o.fabric.as_deref(),
                o.neoforge.as_deref(),
                o.forge.as_deref(),
                &o.source,
            )
        {
            writeln!(err, "failed to resolve Minecraft version: {}", e.name())?;
            return Ok(1);
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (o, out);
        err.write_all(
            b"Rust rewrite instance filesystem adapter is not implemented on this platform\n",
        )?;
        Ok(1)
    }
    #[cfg(any(unix, windows))]
    {
        let root = match Root::open(&o.directory) {
            Ok(r) => r,
            Err(e) => {
                writeln!(
                    err,
                    "failed to open instances directory: {}",
                    super::cli_error_name(&e)
                )?;
                return Ok(1);
            }
        };
        match command {
            "create" => {
                let id = o.id.unwrap();
                let version = o.version.unwrap();
                let manifest = Manifest {
                    schema_version: 1,
                    name: o.name.unwrap_or_else(|| id.clone()),
                    id: id.clone(),
                    version_id: version.clone(),
                    fabric_loader: o.fabric,
                    neoforge_version: o.neoforge,
                    forge_version: o.forge,
                    source: o.source,
                };
                if let Err(e) = root.create(&manifest) {
                    writeln!(
                        err,
                        "failed to create instance: {}",
                        super::cli_error_name(&e)
                    )?;
                    return Ok(1);
                }
                writeln!(
                    out,
                    "instance\t{id}\nversion\t{version}\ndirectory\t{}",
                    super::minecraft_path(&o.directory, &id)
                )?;
            }
            "list" => {
                let manifests = match root.list() {
                    Ok(m) => m,
                    Err(e) => {
                        writeln!(
                            err,
                            "failed to list instances: {}",
                            super::cli_error_name(&e)
                        )?;
                        return Ok(1);
                    }
                };
                if o.json {
                    serde_json::to_writer(&mut *out, &json!({"instances":manifests}))?;
                    out.write_all(b"\n")?;
                } else {
                    for m in manifests {
                        writeln!(out, "{}\t{}\t{}\t{}", m.id, m.version_id, m.source, m.name)?;
                    }
                }
            }
            "show" => {
                let id = o.id.unwrap();
                let m = match root.load(&id) {
                    Ok(m) => m,
                    Err(e) => {
                        writeln!(
                            err,
                            "failed to load instance: {}",
                            super::cli_error_name(&e)
                        )?;
                        return Ok(1);
                    }
                };
                let path = super::minecraft_path(&o.directory, &id);
                if o.json {
                    serde_json::to_writer(&mut *out, &json!({"instance":m,"directory":path}))?;
                    out.write_all(b"\n")?;
                } else {
                    writeln!(
                        out,
                        "instance\t{}\nname\t{}\nversion\t{}\nsource\t{}\ndirectory\t{}",
                        m.id, m.name, m.version_id, m.source, path
                    )?;
                }
            }
            "delete" => {
                let id = o.id.unwrap();
                if let Err(e) = root.delete(&id) {
                    writeln!(
                        err,
                        "failed to delete instance: {}",
                        super::cli_error_name(&e)
                    )?;
                    return Ok(1);
                }
                writeln!(out, "deleted\t{id}")?;
            }
            _ => unreachable!(),
        };
        Ok(0)
    }
}
pub(crate) fn game_path(root: &str, id: &str) -> String {
    std::path::Path::new(root)
        .join(id)
        .join(".minecraft")
        .to_string_lossy()
        .into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn invalid_native_name_keeps_source_error() {
        assert_eq!(
            zig_error(&io::Error::from_raw_os_error(
                rustix::io::Errno::ILSEQ.raw_os_error()
            )),
            "BadPathName"
        );
        assert_eq!(
            zig_error(&io::Error::from_raw_os_error(
                rustix::io::Errno::NOENT.raw_os_error()
            )),
            "FileNotFound"
        );
        assert_eq!(
            zig_error(&io::Error::new(
                io::ErrorKind::InvalidData,
                "FixtureSchemaError"
            )),
            "FixtureSchemaError"
        );
    }
    #[test]
    fn strict_flags() {
        assert!(options("list", &["--wat".into()]).is_none());
        assert!(options("delete", &["a".into(), "--json".into()]).is_none());
        assert!(options("show", &["../a".into()]).is_none());
        assert!(
            !options(
                "create",
                &[
                    "a".into(),
                    "--version".into(),
                    "1.21".into(),
                    "--no-validate".into()
                ]
            )
            .unwrap()
            .validate
        );
    }
}

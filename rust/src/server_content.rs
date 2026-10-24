//! Server content is the existing content engine under a manifest-derived context
//! and the server lifecycle lock. Datapacks belong to the base world, not its
//! Nether/End siblings; their mutable registry moves with that world.
use serde_json::value::RawValue;
use std::io::{self, Write};

pub fn handle(
    method: &str,
    params: Option<&RawValue>,
    id: &str,
    out: &mut impl Write,
) -> Option<io::Result<()>> {
    handle_with(
        method,
        params,
        id,
        out,
        &crate::content::http::Native,
        &crate::content::http::Native,
    )
}
pub fn handle_with<
    M: crate::content::remote::Metadata + crate::content::curseforge::Metadata + Sync,
    T: crate::install::Transport,
>(
    method: &str,
    params: Option<&RawValue>,
    id: &str,
    out: &mut impl Write,
    metadata: &M,
    transport: &T,
) -> Option<io::Result<()>> {
    let rest = method.strip_prefix("server.")?;
    let (family, op) = rest.split_once('.')?;
    if !matches!(family, "mods" | "datapacks")
        || !matches!(
            op,
            "list" | "install" | "set-version" | "enable" | "disable" | "remove" | "adopt"
        )
    {
        return None;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (params, metadata, transport);
        Some(crate::protocol::error(
            out,
            Some(id),
            "UNSUPPORTED_PLATFORM",
            "Server management is currently supported only on Linux",
        ))
    }
    #[cfg(target_os = "linux")]
    {
        Some(linux::run(
            method, family, op, params, id, out, metadata, transport,
        ))
    }
}
pub fn cli(
    args: &[String],
    out: &mut impl Write,
    errors: &mut impl Write,
) -> Option<io::Result<u8>> {
    cli_with(
        args,
        out,
        errors,
        &crate::content::http::Native,
        &crate::content::http::Native,
    )
}
pub fn cli_with<
    M: crate::content::remote::Metadata + crate::content::curseforge::Metadata + Sync,
    T: crate::install::Transport,
>(
    args: &[String],
    out: &mut impl Write,
    errors: &mut impl Write,
    metadata: &M,
    transport: &T,
) -> Option<io::Result<u8>> {
    let family = args.first()?.as_str();
    if !matches!(family, "mods" | "datapacks") {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        Some(linux::cli_run(
            family,
            &args[1..],
            out,
            errors,
            metadata,
            transport,
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (out, metadata, transport);
        Some(
            writeln!(
                errors,
                "Server management is currently supported only on Linux"
            )
            .map(|_| 1),
        )
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::{
        auth::json::Json,
        content::{
            self, Root, curseforge, modrinth,
            registry::{Kind, Provider},
            remote,
        },
        install,
        protocol::{self, RpcError},
        servers::{lifecycle, properties, store},
        store::Store,
    };
    use serde_json::{Value, json};
    use std::fs::File;
    use zeroize::Zeroizing;
    fn err(c: &str, m: impl Into<String>) -> RpcError {
        RpcError::new(c, m)
    }
    fn invalid(m: impl Into<String>) -> RpcError {
        err("INVALID_PARAMS", m)
    }
    struct Params {
        directory: String,
        id: String,
        project: Option<String>,
        file: Option<String>,
        version: Option<String>,
        provider: Option<String>,
        key: Option<Zeroizing<String>>,
        store: String,
        dependencies: bool,
        workers: u8,
        retries: u8,
    }
    fn parse(raw: Option<&RawValue>, family: &str, op: &str) -> Result<Params, RpcError> {
        let what = match op {
            "list" => "listing",
            "enable" | "disable" => "toggle",
            "remove" => "removal",
            "adopt" => "adoption",
            "set-version" => "version change",
            _ => "install",
        };
        let raw = raw
            .filter(|r| r.get().trim() != "null")
            .ok_or_else(|| invalid("missing parameters"))?;
        let parse = || -> Result<Params, ()> {
            let j = Json::parse(raw.get())?;
            let required = |n| j.required(n)?.string().map(|s| s.to_string());
            let optional = |n| {
                j.field(n)?
                    .filter(|v| !v.null())
                    .map(|v| v.string().map(|s| s.to_string()))
                    .transpose()
            };
            let number = |n, d| {
                j.field(n)?
                    .map(|v| v.u64().and_then(|v| u8::try_from(v).map_err(|_| ())))
                    .unwrap_or(Ok(d))
            };
            let network = matches!(op, "install" | "set-version");
            let selector = matches!(op, "enable" | "disable" | "remove");
            Ok(Params {
                directory: required("directory")?,
                id: required("id")?,
                project: if network {
                    Some(required("project")?)
                } else if selector {
                    optional("project")?
                } else {
                    None
                },
                file: if selector { optional("file")? } else { None },
                version: if op == "set-version" {
                    Some(required("version_id")?)
                } else if op == "install" {
                    optional("version_id")?
                } else {
                    None
                },
                provider: if op == "install" {
                    optional("provider")?
                } else {
                    None
                },
                key: if network {
                    j.field("api_key")?
                        .filter(|v| !v.null())
                        .map(|v| v.string())
                        .transpose()?
                } else {
                    None
                },
                store: if network || op == "adopt" {
                    j.field("store_directory")?
                        .map(|v| v.string().map(|s| s.to_string()))
                        .unwrap_or_else(|| Ok(crate::store::DEFAULT_DIRECTORY.into()))?
                } else {
                    String::new()
                },
                dependencies: if op == "install" {
                    j.field("with_dependencies")?
                        .map(|v| serde_json::from_str::<bool>(v.0).map_err(|_| ()))
                        .unwrap_or(Ok(true))?
                } else {
                    true
                },
                workers: if network { number("workers", 8)? } else { 8 },
                retries: if network { number("retries", 2)? } else { 2 },
            })
        };
        let p = parse()
            .map_err(|_| invalid(format!("server {family} {what} parameters are invalid")))?;
        if matches!(op, "install" | "set-version") {
            if p.directory.is_empty()
                || p.id.is_empty()
                || p.project.as_deref() == Some("")
                || (op == "set-version" && p.version.as_deref() == Some(""))
                || p.workers == 0
                || p.workers > 32
                || p.retries > 5
            {
                return Err(invalid(format!(
                    "server {family} {what} parameters are out of range"
                )));
            }
        } else if p.directory.is_empty()
            || p.id.is_empty()
            || (matches!(op, "enable" | "disable" | "remove")
                && p.project.is_some() == p.file.is_some())
        {
            let suffix = if matches!(op, "enable" | "disable" | "remove") {
                "directory, id, and exactly one of project or file"
            } else {
                "directory and id"
            };
            return Err(invalid(format!("server {family} {what} requires {suffix}")));
        }
        Ok(p)
    }
    fn local_name(e: content::Error) -> String {
        match e {
            content::Error::Io(e) => crate::instances::cli_error_name(&e),
            content::Error::Invalid => "ContentSelectorRequired".into(),
            content::Error::Selector(e) | content::Error::Registry(e) => e.into(),
            content::Error::NotFound => "ContentNotFound".into(),
            _ => "Unexpected".into(),
        }
    }
    fn mapped(name: &str, family: &str, op: &str) -> RpcError {
        let (c, m) = match name {
            "ServerNotFound" => ("SERVER_NOT_FOUND", "No server matches the given id"),
            "ServerStatusFailed" => (
                "SERVER_STATUS_FAILED",
                "Failed to validate the server lifecycle status",
            ),
            "ServerAlreadyRunning" => (
                "SERVER_ALREADY_RUNNING",
                "Stop the server before changing its content",
            ),
            "ModLoaderRequired" => (
                "SERVER_LOADER_REQUIRED",
                "The server has no mod loader; mods require Fabric, NeoForge, or Forge",
            ),
            "InvalidServerProperties" => (
                "SERVER_PROPERTIES_INVALID",
                "server.properties is missing or has no usable level-name",
            ),
            "ContentNotFound" => (
                "CONTENT_NOT_FOUND",
                "No registered content matches the given selector",
            ),
            "InvalidModrinthProject"
            | "InvalidModrinthVersionId"
            | "InvalidGameVersion"
            | "InvalidCurseforgeId" => (
                "INVALID_PARAMS",
                "Project, version, or Minecraft version identifiers are invalid",
            ),
            "NoCompatibleContentVersion" => (
                "NO_COMPATIBLE_VERSION",
                "No release of the project supports the server's Minecraft version and loader",
            ),
            "IncompatibleContentVersion" => (
                "INCOMPATIBLE_VERSION",
                "The requested version does not support the server's Minecraft version and loader",
            ),
            "ModrinthVersionProjectMismatch" | "CurseforgeFileProjectMismatch" => (
                "VERSION_PROJECT_MISMATCH",
                "The requested version does not belong to the registered project",
            ),
            "ContentFileConflict" => (
                "CONTENT_CONFLICT",
                "A different registered content entry already occupies the target file",
            ),
            "ContentKindMismatch" => (
                "CONTENT_KIND_MISMATCH",
                "The project type does not match the requested content kind",
            ),
            "UnsupportedProjectType" => (
                "UNSUPPORTED_PROJECT_TYPE",
                "The project type is not supported",
            ),
            "CurseforgeApiKeyRequired" => (
                "INVALID_PARAMS",
                "Switching a curseforge entry requires an api_key supplied by the caller",
            ),
            "CurseforgeDownloadUnavailable" => (
                "DOWNLOAD_UNAVAILABLE",
                "The project file is not available for third-party downloads",
            ),
            "UnsuccessfulHttpStatus" => (
                "HTTP_STATUS_ERROR",
                "The content API returned an unsuccessful HTTP status",
            ),
            "ModrinthResponseTooLarge" | "CurseforgeResponseTooLarge" => (
                "RESPONSE_TOO_LARGE",
                "The content API response exceeds the size limit",
            ),
            "UnsafePath" | "InvalidContentFile" => ("INVALID_PARAMS", "The selector is invalid"),
            _ => {
                let dp = family == "datapacks";
                return match op {
                    "list" => err(
                        "CONTENT_LIST_FAILED",
                        if dp {
                            "Failed to read the server datapack registry"
                        } else {
                            "Failed to read the server content registry"
                        },
                    ),
                    "install" => err(
                        if dp {
                            "SERVER_DATAPACKS_INSTALL_FAILED"
                        } else {
                            "SERVER_CONTENT_INSTALL_FAILED"
                        },
                        if dp {
                            "Failed to install the server datapack"
                        } else {
                            "Failed to install the server mod"
                        },
                    ),
                    "set-version" => err(
                        if dp {
                            "SERVER_DATAPACKS_SET_VERSION_FAILED"
                        } else {
                            "SERVER_CONTENT_SET_VERSION_FAILED"
                        },
                        if dp {
                            "Failed to change the server datapack version"
                        } else {
                            "Failed to change the server mod version"
                        },
                    ),
                    "remove" => err(
                        if dp {
                            "SERVER_DATAPACKS_REMOVE_FAILED"
                        } else {
                            "SERVER_CONTENT_REMOVE_FAILED"
                        },
                        if dp {
                            "Failed to remove the server datapack entry"
                        } else {
                            "Failed to remove the server content entry"
                        },
                    ),
                    "adopt" => err(
                        if dp {
                            "SERVER_DATAPACKS_ADOPT_FAILED"
                        } else {
                            "SERVER_CONTENT_ADOPT_FAILED"
                        },
                        if dp {
                            "Failed to adopt unmanaged server datapacks"
                        } else {
                            "Failed to adopt unmanaged server content"
                        },
                    ),
                    _ => err(
                        if dp {
                            "SERVER_DATAPACKS_TOGGLE_FAILED"
                        } else {
                            "SERVER_CONTENT_TOGGLE_FAILED"
                        },
                        if dp {
                            "Failed to toggle the server datapack entry"
                        } else {
                            "Failed to toggle the server content entry"
                        },
                    ),
                };
            }
        };
        err(c, m)
    }
    fn target(
        p: &Params,
        require_loader: bool,
    ) -> Result<(File, String, Option<&'static str>), String> {
        let root = store::open_root(&p.directory).map_err(|_| "ServerNotFound")?;
        let m = store::load(&root, &p.id).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
                "ServerNotFound".into()
            } else {
                e.to_string()
            }
        })?;
        let loader = if m.fabric_loader.is_some() {
            Some("fabric")
        } else if m.neoforge_version.is_some() {
            Some("neoforge")
        } else if m.forge_version.is_some() {
            Some("forge")
        } else {
            None
        };
        if require_loader && loader.is_none() {
            return Err("ModLoaderRequired".into());
        }
        let dir = store::open_server(&root, &p.id).map_err(|_| "ServerNotFound")?;
        Ok((dir, m.version_id, loader))
    }
    fn world(dir: &File, create: bool) -> Result<Option<File>, String> {
        use rustix::fs::{self, Mode, OFlags};
        let bytes = lifecycle::read(dir, "server.properties", properties::MAX_FILE_BYTES)
            .map_err(|_| "InvalidServerProperties")?;
        let doc = properties::parse(&bytes).map_err(|_| "InvalidServerProperties")?;
        let name = doc.get("level-name").ok_or("InvalidServerProperties")?;
        crate::worlds::validate_name(name).map_err(|_| "InvalidServerProperties")?;
        if create {
            match fs::mkdirat(dir, name, Mode::from_raw_mode(0o755)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                Err(e) => return Err(e.to_string()),
            }
        }
        match lifecycle::open(dir, name, OFlags::RDONLY | OFlags::DIRECTORY, 0) {
            Ok(d) => Ok(Some(d)),
            Err(e) if e.kind() == io::ErrorKind::NotFound && !create => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(
        method: &str,
        family: &str,
        op: &str,
        raw: Option<&RawValue>,
        id: &str,
        out: &mut impl Write,
        metadata: &M,
        transport: &T,
    ) -> io::Result<()> {
        let prepare = || -> Result<_, RpcError> {
            let p = parse(raw, family, op)?;
            let provider = if op == "install" {
                match p.provider.as_deref() {
                    None | Some("modrinth") => Provider::Modrinth,
                    Some("curseforge") => Provider::Curseforge,
                    _ => return Err(invalid("provider must be modrinth or curseforge")),
                }
            } else {
                Provider::Modrinth
            };
            if op == "install" {
                match provider {
                    Provider::Curseforge => {
                        let k = p.key.as_deref().ok_or_else(|| {
                            invalid("curseforge installs require an api_key supplied by the caller")
                        })?;
                        crate::modpack::provider::validate_key(k)
                            .map_err(|_| invalid("the curseforge api_key is invalid"))?;
                    }
                    Provider::Modrinth if p.key.is_some() => {
                        return Err(invalid("api_key is only used with the curseforge provider"));
                    }
                    _ => (),
                }
            }
            if op == "set-version"
                && let Some(k) = p.key.as_deref()
            {
                crate::modpack::provider::validate_key(k)
                    .map_err(|_| invalid("the curseforge api_key is invalid"))?;
            }
            let cas =
                if matches!(op, "install" | "set-version" | "adopt") {
                    Some(Store::open(&p.store).map_err(|_| {
                        err("STORE_ROOT_ERROR", "Failed to open the artifact store")
                    })?)
                } else {
                    None
                };
            let (dir, game, loader) = target(&p, family == "mods" && op == "install")
                .map_err(|e| mapped(&e, family, op))?;
            let guard = if op != "list" {
                Some(lifecycle::lock_stopped(&dir).map_err(|e| mapped(e, family, op))?)
            } else {
                None
            };
            let content_dir = if family == "datapacks" {
                world(&dir, op != "list").map_err(|e| mapped(&e, family, op))?
            } else {
                Some(dir)
            };
            Ok((
                p,
                provider,
                cas,
                game,
                if family == "datapacks" { None } else { loader },
                guard,
                content_dir.map(Root::from_directory),
            ))
        };
        let (p, provider, cas, game, loader, _guard, root) = match prepare() {
            Ok(v) => v,
            Err(e) => return protocol::error(out, Some(id), &e.code, &e.message),
        };
        let kind = if family == "mods" {
            Kind::Mod
        } else {
            Kind::Datapack
        };
        let fields: Result<Value, String> = if matches!(op, "install" | "set-version") {
            let root = root.as_ref().unwrap();
            let cas = cas.as_ref().unwrap();
            let options = install::Options {
                workers: p.workers,
                retries: p.retries,
                sync_files: true,
            };
            install::stream::stream(
                |reporter| {
                    network(
                        &p, op, kind, root, cas, &game, loader, provider, metadata, transport,
                        options, reporter,
                    )
                },
                |event| crate::modpack::rpc::write_event(out, id, event),
            )?
            .map_err(|e| e.name().to_owned())
        } else {
            local(&p, op, kind, root.as_ref(), cas.as_ref()).map_err(local_name)
        };
        match fields {
            Err(e) => {
                let e = mapped(&e, family, op);
                protocol::error(out, Some(id), &e.code, &e.message)
            }
            Ok(Value::Object(fields)) => {
                let mut result = serde_json::Map::new();
                result.insert("stage".into(), method.into());
                result.insert("id".into(), p.id.into());
                result.extend(fields);
                serde_json::to_writer(
                    &mut *out,
                    &json!({"protocol":1,"id":id,"event":"result","result":result}),
                )?;
                out.write_all(b"\n")
            }
            _ => unreachable!(),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn network<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(
        p: &Params,
        op: &str,
        kind: Kind,
        root: &Root,
        cas: &Store,
        game: &str,
        loader: Option<&str>,
        provider: Provider,
        metadata: &M,
        transport: &T,
        options: install::Options,
        reporter: &(dyn Fn(install::Event) -> Result<(), install::Error> + Sync),
    ) -> Result<Value, install::Error> {
        if op == "set-version" {
            let v = root.set_version(
                cas,
                kind,
                game,
                loader,
                p.project.as_deref().unwrap(),
                p.version.as_deref().unwrap(),
                p.key.as_deref().map(|k| k.as_str()),
                metadata,
                transport,
                options,
                Some(reporter),
            )?;
            return Ok(
                json!({"changed":v.changed,"file":v.file,"sha1":v.sha1,"size":v.size,"old_version_id":v.old_version_id,"new_version_id":v.new_version_id}),
            );
        }
        let project = p.project.as_deref().unwrap();
        let ids = (|| -> modrinth::Result<()> {
            match provider {
                Provider::Modrinth => modrinth::project_ref(project)?,
                _ => curseforge::id(project)?,
            };
            if provider == Provider::Curseforge {
                crate::modpack::provider::validate_key(p.key.as_deref().unwrap())
                    .map_err(|e| modrinth::Error::from(e.name()))?;
            }
            modrinth::game_version(game)?;
            if let Some(v) = p.version.as_deref() {
                match provider {
                    Provider::Modrinth => modrinth::version_id(v)?,
                    _ => curseforge::id(v)?,
                }
            }
            Ok(())
        })();
        ids.map_err(|e| install::Error::named(&e.0))?;
        let registry = root
            .load()
            .map_err(|e| install::Error::named(&local_name(e)))?;
        let r = match provider {
            Provider::Modrinth => remote::resolve_modrinth(
                metadata,
                kind,
                game,
                loader,
                project,
                p.version.as_deref(),
                p.dependencies,
                &registry,
            ),
            _ => curseforge::resolve(
                metadata,
                kind,
                game,
                loader,
                p.key.as_deref().unwrap(),
                project,
                p.version.as_deref(),
                p.dependencies,
                &registry,
            ),
        }
        .map_err(|e| install::Error::named(&e.0))?;
        let s = root.install_resolved_with_registry(
            cas,
            kind,
            game,
            provider,
            r,
            registry,
            options,
            transport,
            Some(reporter),
        )?;
        Ok(json!({"installed":s.installed,"skipped":s.skipped}))
    }
    fn local(
        p: &Params,
        op: &str,
        kind: Kind,
        root: Option<&Root>,
        cas: Option<&Store>,
    ) -> Result<Value, content::Error> {
        let Some(root) = root else {
            return Ok(json!({"entries": [], "unmanaged": []}));
        };
        match op {
            "list" => {
                let (entries, unmanaged) = root.list(kind)?;
                Ok(json!({"entries": entries, "unmanaged": unmanaged}))
            }
            "remove" => {
                Ok(json!({"removed": root.remove(kind, p.project.as_deref(), p.file.as_deref())?}))
            }
            "adopt" => Ok(json!({"adopted": root.adopt(kind, cas.unwrap())?})),
            _ => {
                let (changed, entry) = root.toggle(
                    kind,
                    p.project.as_deref(),
                    p.file.as_deref(),
                    op == "enable",
                )?;
                Ok(json!({"changed": changed, "entry": entry}))
            }
        }
    }
    include!("server_content_cli.rs");
}

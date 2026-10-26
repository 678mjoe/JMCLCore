//! Content install streaming with borrowed request spans and wiping API keys.
use super::{
    Root, curseforge,
    registry::{Kind, Provider},
    remote,
};
use crate::{auth::json::Json, install, protocol, store::Store};
use serde::Serialize;
use serde_json::value::RawValue;
use std::io::{self, Write};
use zeroize::Zeroizing;
struct Params {
    directory: String,
    world: Option<String>,
    game: String,
    loader: Option<String>,
    provider: Option<String>,
    key: Option<Zeroizing<String>>,
    project: String,
    version: Option<String>,
    dependencies: bool,
    store: String,
    workers: u8,
    retries: u8,
}
fn text(j: Json<'_>) -> Result<Zeroizing<String>, ()> {
    if j.0.starts_with('[') {
        let mut bytes = Zeroizing::new(Vec::new());
        for b in j.array()? {
            bytes.push(u8::try_from(b.u64()?).map_err(|_| ())?)
        }
        let s = std::str::from_utf8(&bytes).map_err(|_| ())?;
        Ok(Zeroizing::new(s.to_owned()))
    } else {
        j.string()
    }
}
fn optional(j: Json<'_>, name: &str) -> Result<Option<String>, ()> {
    j.field(name)?
        .filter(|v| !v.null())
        .map(|v| text(v).map(|s| s.to_string()))
        .transpose()
}
fn parse(raw: &RawValue, dp: bool, change: bool) -> Result<Params, ()> {
    let j = Json::parse(raw.get())?;
    let required = |n| text(j.required(n)?).map(|s| s.to_string());
    let number = |n, default| -> Result<u8, ()> {
        j.field(n)?
            .map(|v| {
                crate::json_number::dynamic_raw(v.0, 8)
                    .map(|n| n as u8)
                    .map_err(|_| ())
            })
            .unwrap_or(Ok(default))
    };
    Ok(Params {
        directory: required("directory")?,
        world: if dp { Some(required("world")?) } else { None },
        game: required("minecraft_version")?,
        loader: optional(j, "loader")?,
        provider: if change {
            None
        } else {
            optional(j, "provider")?
        },
        key: j
            .field("api_key")?
            .filter(|v| !v.null())
            .map(|v| text(v))
            .transpose()?,
        project: required("project")?,
        version: if change {
            Some(required("version_id")?)
        } else {
            optional(j, "version_id")?
        },
        dependencies: if change {
            true
        } else {
            j.field("with_dependencies")?
                .map(|v| serde_json::from_str::<bool>(v.0).map_err(|_| ()))
                .unwrap_or(Ok(true))?
        },
        store: j
            .field("store_directory")?
            .map(|v| text(v).map(|s| s.to_string()))
            .unwrap_or_else(|| Ok(crate::store::DEFAULT_DIRECTORY.into()))?,
        workers: number("workers", 8)?,
        retries: number("retries", 2)?,
    })
}
fn mapped(name: &str, dp: bool) -> (&'static str, &'static str) {
    match name {
        "ModrinthVersionProjectMismatch" | "CurseforgeFileProjectMismatch" if dp => (
            "VERSION_PROJECT_MISMATCH",
            "The requested version does not belong to the registered project",
        ),
        "UnsafePath" | "InvalidContentFile" if dp => ("INVALID_PARAMS", "The selector is invalid"),
        "WorldNotFound" => ("WORLD_NOT_FOUND", "No world matches the given name"),
        "WorldLocked" => ("WORLD_LOCKED", "The world is in use by a running game"),
        "InvalidModrinthProject"
        | "InvalidModrinthVersionId"
        | "InvalidGameVersion"
        | "InvalidCurseforgeId" => (
            "INVALID_PARAMS",
            "Project, version, or Minecraft version identifiers are invalid",
        ),
        "NoCompatibleContentVersion" => (
            "NO_COMPATIBLE_VERSION",
            if dp {
                "No release of the project supports the requested Minecraft version"
            } else {
                "No release of the project supports the requested Minecraft version and loader"
            },
        ),
        "IncompatibleContentVersion" => (
            "INCOMPATIBLE_VERSION",
            if dp {
                "The requested version does not support the Minecraft version"
            } else {
                "The requested version does not support the Minecraft version and loader"
            },
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
        _ => (
            "CONTENT_INSTALL_FAILED",
            if dp {
                "Failed to install the datapack"
            } else {
                "Failed to install the project"
            },
        ),
    }
}
#[derive(Serialize)]
struct Envelope<'a, T> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    result: T,
}
#[derive(Serialize)]
struct ResultBody<'a> {
    stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    world: Option<&'a str>,
    installed: Vec<remote::InstalledRecord<super::ListedPath>>,
    skipped: usize,
}
#[allow(clippy::too_many_arguments)]
pub fn handle_with<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(
    method: &str,
    params: Option<&RawValue>,
    id: &str,
    out: &mut impl Write,
    metadata: &M,
    transport: &T,
) -> Option<io::Result<()>> {
    let (family, op) = method.split_once('.')?;
    let kind = Kind::from_directory(family)?;
    if !matches!(op, "install" | "set-version") {
        return None;
    }
    Some(run(
        kind,
        params,
        id,
        out,
        metadata,
        transport,
        op == "set-version",
    ))
}
fn run<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(
    kind: Kind,
    raw: Option<&RawValue>,
    id: &str,
    out: &mut impl Write,
    metadata: &M,
    transport: &T,
    change: bool,
) -> io::Result<()> {
    let dp = kind == Kind::Datapack;
    let noun = if dp { "datapack" } else { "content" };
    let operation = if change { "version change" } else { "install" };
    let command = if change { "set-version" } else { "install" };
    let invalid =
        |out: &mut _, message: &str| protocol::error(out, Some(id), "INVALID_PARAMS", message);
    let Some(raw) = raw.filter(|r| r.get().trim() != "null") else {
        return invalid(
            out,
            if change && dp {
                "datapack version change requires directory, world, minecraft_version, project, and version_id"
            } else if change {
                "content version change requires directory, minecraft_version, project, and version_id"
            } else if dp {
                "datapack install requires directory, world, minecraft_version, and project"
            } else {
                "content install requires directory, minecraft_version, and project"
            },
        );
    };
    let p = match parse(raw, dp, change) {
        Ok(p) => p,
        Err(()) => return invalid(out, &format!("{noun} {operation} parameters are invalid")),
    };
    if p.directory.is_empty()
        || p.world.as_ref().is_some_and(|w| w.is_empty())
        || p.game.is_empty()
        || p.project.is_empty()
        || (change && p.version.as_ref().is_none_or(|v| v.is_empty()))
        || p.workers == 0
        || p.workers > 32
        || p.retries > 5
    {
        return invalid(
            out,
            &format!("{noun} {operation} parameters are out of range"),
        );
    }
    let loader = match kind {
        Kind::Mod => {
            let Some(l) = p.loader.as_deref() else {
                return invalid(out, &format!("mods.{command} requires a loader"));
            };
            if !matches!(l, "fabric" | "forge" | "neoforge") {
                return invalid(
                    out,
                    &format!("mods.{command} requires a fabric, forge, or neoforge loader"),
                );
            }
            Some(l)
        }
        Kind::Resourcepack => {
            if p.loader.as_deref().is_some_and(|l| l != "minecraft") {
                return invalid(
                    out,
                    &format!("resourcepacks.{command} does not accept a mod loader"),
                );
            }
            None
        }
        _ => {
            if p.loader.is_some() {
                return invalid(
                    out,
                    &format!("{}.{command} does not accept a loader", kind.directory()),
                );
            }
            None
        }
    };
    let provider = match p.provider.as_deref().unwrap_or("modrinth") {
        "modrinth" => Provider::Modrinth,
        "curseforge" => Provider::Curseforge,
        _ => return invalid(out, "provider must be modrinth or curseforge"),
    };
    if change {
        if p.key
            .as_deref()
            .is_some_and(|k| crate::modpack::provider::validate_key(k).is_err())
        {
            return invalid(out, "the curseforge api_key is invalid");
        }
    } else if provider == Provider::Curseforge {
        let Some(key) = p.key.as_deref() else {
            return invalid(
                out,
                "curseforge installs require an api_key supplied by the caller",
            );
        };
        if crate::modpack::provider::validate_key(key).is_err() {
            return invalid(out, "the curseforge api_key is invalid");
        }
    } else if p.key.is_some() {
        return invalid(out, "api_key is only used with the curseforge provider");
    }
    let store_error = |out: &mut _| {
        protocol::error(
            out,
            Some(id),
            "STORE_ROOT_ERROR",
            "Failed to open the artifact store",
        )
    };
    let (root, store) = if dp {
        let store = match Store::open(&p.store) {
            Ok(s) => s,
            Err(_) => return store_error(out),
        };
        let root = match Root::world(&p.directory, p.world.as_deref().unwrap(), true) {
            Ok(r) => r,
            Err(e) => {
                let (c, m) = if change {
                    mapped_change(&super::cli::name(&e), dp)
                } else {
                    mapped(&super::cli::name(&e), dp)
                };
                return protocol::error(out, Some(id), c, m);
            }
        };
        (root, store)
    } else {
        let root = match Root::open(&p.directory) {
            Ok(r) => r,
            Err(_) => {
                return protocol::error(
                    out,
                    Some(id),
                    "INSTALL_ROOT_ERROR",
                    "Failed to open the install root",
                );
            }
        };
        let store = match Store::open(&p.store) {
            Ok(s) => s,
            Err(_) => return store_error(out),
        };
        (root, store)
    };
    if change {
        let result = install::stream::stream(
            |reporter| {
                root.set_version_bytes(
                    &store,
                    kind,
                    &p.game,
                    loader,
                    &p.project,
                    p.version.as_deref().unwrap(),
                    p.key.as_deref().map(|k| k.as_str()),
                    metadata,
                    transport,
                    install::Options {
                        workers: p.workers,
                        retries: p.retries,
                        sync_files: true,
                    },
                    Some(reporter),
                )
            },
            |event| crate::content::rpc::write_byte_event(out, id, event),
        )?;
        return match result {
            Err(e) => {
                let (c, m) = mapped_change(e.name(), dp);
                protocol::error(out, Some(id), c, m)
            }
            Ok(change) => {
                #[derive(Serialize)]
                struct Changed<'a> {
                    stage: String,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    world: Option<&'a str>,
                    #[serde(flatten)]
                    change: remote::ByteVersionChange,
                }
                serde_json::to_writer(
                    &mut *out,
                    &Envelope {
                        protocol: 1,
                        id,
                        event: "result",
                        result: Changed {
                            stage: format!("{}.set-version", kind.directory()),
                            world: p.world.as_deref(),
                            change,
                        },
                    },
                )?;
                out.write_all(b"\n")
            }
        };
    }
    let result = install::stream::stream(
        |reporter| {
            let identifiers = (|| -> super::modrinth::Result<()> {
                match provider {
                    Provider::Modrinth => super::modrinth::project_ref(&p.project)?,
                    Provider::Curseforge => curseforge::id(&p.project)?,
                    Provider::Manual => unreachable!(),
                }
                super::modrinth::game_version(&p.game)?;
                if let Some(v) = p.version.as_deref() {
                    match provider {
                        Provider::Modrinth => super::modrinth::version_id(v)?,
                        Provider::Curseforge => curseforge::id(v)?,
                        Provider::Manual => unreachable!(),
                    }
                }
                Ok(())
            })();
            identifiers.map_err(|e| install::Error::named(&e.0))?;
            let registry = root
                .load_listing()
                .map_err(|e| install::Error::named(&super::cli::name(&e)))?;
            let r = match provider {
                Provider::Modrinth => remote::resolve_modrinth_bytes(
                    metadata,
                    kind,
                    &p.game,
                    loader,
                    &p.project,
                    p.version.as_deref(),
                    p.dependencies,
                    &registry,
                ),
                Provider::Curseforge => curseforge::resolve_bytes(
                    metadata,
                    kind,
                    &p.game,
                    loader,
                    p.key.as_deref().unwrap(),
                    &p.project,
                    p.version.as_deref(),
                    p.dependencies,
                    &registry,
                ),
                Provider::Manual => unreachable!(),
            }
            .map_err(|e| install::Error::named(&e.0))?;
            root.install_resolved_with_registry_bytes(
                &store,
                kind,
                &p.game,
                provider,
                r,
                registry,
                install::Options {
                    workers: p.workers,
                    retries: p.retries,
                    sync_files: true,
                },
                transport,
                Some(reporter),
            )
        },
        |event| crate::content::rpc::write_byte_event(out, id, event),
    )?;
    match result {
        Err(e) => {
            let (c, m) = mapped(e.name(), dp);
            protocol::error(out, Some(id), c, m)
        }
        Ok(s) => {
            serde_json::to_writer(
                &mut *out,
                &Envelope {
                    protocol: 1,
                    id,
                    event: "result",
                    result: ResultBody {
                        stage: format!("{}.install", kind.directory()),
                        world: p.world.as_deref(),
                        installed: s.installed,
                        skipped: s.skipped,
                    },
                },
            )?;
            out.write_all(b"\n")
        }
    }
}
/// Production entry: no credential enters artifact transport or generic JSON.
/// Integration must call before Value-only local content dispatch.
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
        &super::http::Native,
        &super::http::Native,
    )
}

fn mapped_change(name: &str, dp: bool) -> (&'static str, &'static str) {
    match name {
        "ContentNotFound" => (
            "CONTENT_NOT_FOUND",
            if dp {
                "No registered content matches the given selector"
            } else {
                "No registered content matches the given project"
            },
        ),
        "ModrinthVersionProjectMismatch" | "CurseforgeFileProjectMismatch" => (
            "VERSION_PROJECT_MISMATCH",
            "The requested version does not belong to the registered project",
        ),
        "CurseforgeApiKeyRequired" => (
            "INVALID_PARAMS",
            "Switching a curseforge entry requires an api_key supplied by the caller",
        ),
        "InvalidModrinthProject"
        | "InvalidModrinthVersionId"
        | "InvalidGameVersion"
        | "InvalidCurseforgeId"
        | "IncompatibleContentVersion"
        | "ContentFileConflict"
        | "CurseforgeDownloadUnavailable"
        | "UnsuccessfulHttpStatus" => mapped(name, dp),
        "WorldNotFound" | "WorldLocked" if dp => mapped(name, dp),
        _ => (
            "CONTENT_SET_VERSION_FAILED",
            if dp {
                "Failed to change the datapack version"
            } else {
                "Failed to change the content version"
            },
        ),
    }
}

pub(crate) fn write_byte_event(
    out: &mut impl Write,
    id: &str,
    event: install::ByteEvent,
) -> io::Result<()> {
    #[derive(Serialize)]
    struct Frame<'a> {
        protocol: u32,
        id: &'a str,
        event: &'static str,
        source: &'static str,
        #[serde(flatten)]
        payload: serde_json::Value,
    }
    let (event, payload) = match event {
        install::ByteEvent::File(file) => ("file", serde_json::json!({"file":file})),
        install::ByteEvent::Retry(retry) => ("retry", serde_json::json!({"retry":retry})),
        install::ByteEvent::Progress(progress) => {
            ("progress", serde_json::json!({"progress":progress}))
        }
    };
    serde_json::to_writer(
        &mut *out,
        &Frame {
            protocol: 1,
            id,
            event,
            source: "official",
            payload,
        },
    )?;
    out.write_all(b"\n")?;
    out.flush()
}

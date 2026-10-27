//! Conditional vanilla execute path. Loader requests retain the established route.
use super::*;
use crate::{
    auth::json::Json,
    metadata::Source,
    plan::{CompletionFailure, Target},
    protocol::{self, RpcError},
    version,
};
use serde_json::value::RawValue;
#[derive(Debug)]
pub(crate) struct Request {
    id: Vec<u8>,
    directory: Vec<u8>,
    store: Vec<u8>,
    source: Source,
    os: Option<String>,
    arch: Option<String>,
    workers: u8,
    retries: u8,
}
pub(crate) fn selected(raw: Option<&RawValue>) -> bool {
    let Some(raw) = raw else { return false };
    let Ok(p) = Json::parse(raw.get()) else {
        return false;
    };
    ["fabric_loader", "neoforge_version", "forge_version"]
        .iter()
        .all(|key| {
            matches!(p.field(key), Ok(None)) || matches!(p.field(key),Ok(Some(v)) if v.null())
        })
}
fn bytes(v: Json<'_>) -> Result<Vec<u8>, ()> {
    if v.0.starts_with('"') {
        return Ok(v.string()?.as_bytes().to_vec());
    }
    v.array()?
        .into_iter()
        .map(|v| {
            crate::json_number::dynamic_raw(v.0, 8)
                .map(|n| n as u8)
                .map_err(|_| ())
        })
        .collect()
}
fn tag(v: Json<'_>, tags: &[&str]) -> Result<String, ()> {
    let value = serde_json::from_str(v.0).map_err(|_| ())?;
    version::enum_tag(&value, tags)
        .map(|i| tags[i].to_owned())
        .ok_or(())
}
pub(crate) fn parse(raw: Option<&RawValue>) -> Result<Request, RpcError> {
    let missing = || RpcError::new("INVALID_PARAMS", "install.execute requires an id");
    let bad = || RpcError::new("INVALID_PARAMS", "install.execute parameters are invalid");
    let raw = raw
        .filter(|v| v.get().trim() != "null")
        .ok_or_else(missing)?;
    let p = Json::parse(raw.get()).map_err(|_| bad())?;
    let parsed = (|| -> Result<_, ()> {
        let id = bytes(p.required("id")?)?;
        // Full declared schema before semantic/range checks, including unused Java fields.
        for key in ["fabric_loader", "neoforge_version", "forge_version"] {
            if let Some(v) = p.field(key)?
                && !v.null()
            {
                bytes(v)?;
                return Err(());
            }
        }
        let default_bytes = |key, default: &[u8]| {
            p.field(key)?
                .map(bytes)
                .unwrap_or_else(|| Ok(default.to_vec()))
        };
        let directory = default_bytes("directory", b".minecraft")?;
        let store = default_bytes("store_directory", b".jmclcore")?;
        let source = match p.field("source")? {
            None => Source::Official,
            Some(v) => Source::parse(&tag(v, &["official", "bmclapi"])?).ok_or(())?,
        };
        let optional = |key, tags: &[&str]| match p.field(key)? {
            None => Ok(None),
            Some(v) if v.null() => Ok(None),
            Some(v) => tag(v, tags).map(Some),
        };
        let os = optional("os", &["windows", "linux", "osx"])?;
        let arch = optional("arch", &["x86", "x86_64", "arm", "arm64"])?;
        let number = |key, default| {
            p.field(key)?
                .map(|v| {
                    crate::json_number::dynamic_raw(v.0, 8)
                        .map(|n| n as u8)
                        .map_err(|_| ())
                })
                .unwrap_or(Ok(default))
        };
        let workers = number("workers", 8)?;
        let retries = number("retries", 2)?;
        if let Some(paths) = p.field("java_paths")? {
            for v in paths.array()? {
                bytes(v)?;
            }
        }
        let policy = default_bytes("java_policy", b"auto")?;
        Ok((
            Request {
                id,
                directory,
                store,
                source,
                os,
                arch,
                workers,
                retries,
            },
            policy,
        ))
    })()
    .map_err(|_| bad())?;
    let (p, policy) = parsed;
    if p.id.is_empty()
        || p.directory.is_empty()
        || p.store.is_empty()
        || p.workers == 0
        || p.workers > 32
        || p.retries > 5
    {
        return Err(RpcError::new(
            "INVALID_PARAMS",
            "install.execute parameters are out of range",
        ));
    }
    if !matches!(policy.as_slice(), b"auto" | b"local" | b"managed") {
        return Err(RpcError::new(
            "INVALID_PARAMS",
            "java_policy must be auto, local, or managed",
        ));
    }
    Ok(p)
}
fn error(out: &mut impl Write, id: &str, e: RpcError) -> io::Result<()> {
    protocol::error(out, Some(id), &e.code, &e.message)
}
pub(crate) fn run<M: version::Transport, T: Transport>(
    out: &mut impl Write,
    id: &str,
    p: Request,
    svc: &version::Service<M>,
    files: &T,
) -> io::Result<()> {
    let mut target = match Target::host() {
        Ok(t) => t,
        Err(_) => {
            return protocol::error(
                out,
                Some(id),
                "UNSUPPORTED_PLATFORM",
                "The host platform is not supported",
            );
        }
    };
    if let Some(os) = p.os {
        target.os = os
    }
    if let Some(arch) = p.arch {
        target.arch = arch
    }
    let plan = match svc.vanilla_read_plan(&p.id, p.source, target) {
        Ok(p) => p,
        Err(e) => return error(out, id, super::rpc::planning_error(e)),
    };
    let phase = plan
        .vanilla_download_phase()
        .expect("vanilla builder has no loader");
    match phase.complete(
        id,
        &p.directory,
        &p.store,
        Options {
            workers: p.workers,
            retries: p.retries,
            sync_files: true,
        },
        files,
        out,
    ) {
        Ok(_) => Ok(()),
        Err(CompletionFailure::Output(e)) => Err(e),
        Err(e) => {
            let (code, message) = match e {
                CompletionFailure::Root => {
                    ("INSTALL_ROOT_ERROR", "Failed to open the install root")
                }
                CompletionFailure::Store => {
                    ("STORE_ROOT_ERROR", "Failed to open the artifact store")
                }
                CompletionFailure::Files(_) => (
                    "DOWNLOAD_FAILED",
                    "A planned file could not be downloaded and verified",
                ),
                CompletionFailure::Receipt(_) => (
                    "INSTALL_STATE_ERROR",
                    "Failed to record the completed installation",
                ),
                CompletionFailure::Output(_) => unreachable!(),
            };
            protocol::error(out, Some(id), code, message)
        }
    }
}
#[cfg(test)]
pub(crate) fn handle_with<M: version::Transport, T: Transport>(
    out: &mut impl Write,
    id: &str,
    raw: Option<&RawValue>,
    svc: &version::Service<M>,
    files: &T,
) -> Option<io::Result<()>> {
    if !selected(raw) {
        return None;
    }
    Some(match parse(raw) {
        Ok(p) => run(out, id, p, svc, files),
        Err(e) => error(out, id, e),
    })
}
#[cfg(test)]
mod tests;

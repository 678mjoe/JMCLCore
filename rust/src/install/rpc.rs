use super::{
    service::{Planner, Selection},
    *,
};
use crate::{
    metadata::{self, Source},
    protocol::{self, RpcError},
    version,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::io::{self, Write};
#[derive(Clone)]
pub(crate) struct PathValue(Vec<u8>);
impl From<&str> for PathValue {
    fn from(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}
impl From<String> for PathValue {
    fn from(s: String) -> Self {
        Self(s.into_bytes())
    }
}
impl PathValue {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn path(&self) -> io::Result<std::path::PathBuf> {
        crate::store::path_from_bytes(&self.0).map(Into::into)
    }
}
impl<'de> Deserialize<'de> for PathValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        if let Some(s) = v.as_str() {
            return Ok(Self::from(s));
        }
        let a = v
            .as_array()
            .ok_or_else(|| serde::de::Error::custom("Expected path bytes"))?;
        a.iter()
            .map(|v| {
                metadata::unsigned(v, 255)
                    .map(|v| v as u8)
                    .map_err(|e| serde::de::Error::custom(e.name()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}
fn directory() -> PathValue {
    ".minecraft".into()
}
fn store() -> PathValue {
    ".jmclcore".into()
}
fn policy() -> String {
    "auto".into()
}
fn workers() -> u8 {
    8
}
fn retries() -> u8 {
    2
}
fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    metadata::unsigned(&v, 255)
        .map(|v| v as u8)
        .map_err(|e| serde::de::Error::custom(e.name()))
}
#[derive(Deserialize)]
pub(crate) struct PrepareParams {
    #[serde(flatten)]
    pub selection: Selection,
    #[serde(default = "directory")]
    pub directory: PathValue,
}
#[derive(Deserialize)]
pub(crate) struct ExecuteParams {
    #[serde(flatten)]
    pub prepare: PrepareParams,
    #[serde(default = "store")]
    pub store_directory: PathValue,
    #[serde(default = "workers", deserialize_with = "number")]
    pub workers: u8,
    #[serde(default = "retries", deserialize_with = "number")]
    pub retries: u8,
    #[serde(default)]
    pub java_paths: Vec<String>,
    #[serde(default = "policy")]
    pub java_policy: String,
}
pub(crate) enum Request {
    Execute(ExecuteParams),
    Prepare(PrepareParams),
}
fn normalize(method: &str, raw: &str) -> Result<serde_json::Value, ()> {
    use serde_json::Value;
    fn text(v: &mut Value) -> Result<(), ()> {
        if v.is_string() {
            return Ok(());
        }
        let a = v.as_array().ok_or(())?;
        let b = a
            .iter()
            .map(|v| metadata::unsigned(v, 255).map(|v| v as u8).map_err(|_| ()))
            .collect::<Result<Vec<_>, _>>()?;
        *v = String::from_utf8(b).map_err(|_| ())?.into();
        Ok(())
    }
    fn tag(v: &mut Value, tags: &[&str]) -> Result<(), ()> {
        if v.as_str().is_some_and(|s| tags.contains(&s)) {
            return Ok(());
        }
        let n = if let Some(s) = v.as_str() {
            if s == "-0" || s.contains(['.', 'e', 'E']) {
                return Err(());
            }
            let body = s.strip_prefix(['+', '-']).unwrap_or(s);
            if body.is_empty() || body.starts_with('_') || body.ends_with('_') {
                return Err(());
            }
            usize::try_from(s.replace('_', "").parse::<i64>().map_err(|_| ())?).map_err(|_| ())?
        } else {
            v.as_u64().ok_or(())? as usize
        };
        *v = tags.get(n).ok_or(())?.to_string().into();
        Ok(())
    }
    let mut value: Value = serde_json::from_str(raw).map_err(|_| ())?;
    let p = value.as_object_mut().ok_or(())?;
    if let Some(v) = p.get_mut("id") {
        text(v)?;
    }
    for key in ["fabric_loader", "neoforge_version", "forge_version"] {
        if let Some(v) = p.get_mut(key).filter(|v| !v.is_null()) {
            text(v)?;
        }
    }
    if let Some(v) = p.get_mut("source") {
        tag(v, &["official", "bmclapi"])?;
    }
    if let Some(v) = p.get_mut("os").filter(|v| !v.is_null()) {
        tag(v, &["windows", "linux", "osx"])?;
    }
    if let Some(v) = p.get_mut("arch").filter(|v| !v.is_null()) {
        tag(v, &["x86", "x86_64", "arm", "arm64"])?;
    }
    if method == "install.execute" {
        if let Some(v) = p.get_mut("java_policy") {
            text(v)?;
        }
        if let Some(v) = p.get_mut("java_paths") {
            for v in v.as_array_mut().ok_or(())? {
                text(v)?;
            }
        }
    }
    Ok(value)
}
pub(crate) fn parse(method: &str, raw: Option<&RawValue>) -> Result<Request, RpcError> {
    let Some(raw) = raw.filter(|v| v.get().trim() != "null") else {
        return Err(RpcError::new(
            "INVALID_PARAMS",
            format!("{method} requires an id"),
        ));
    };
    let invalid = || RpcError::new("INVALID_PARAMS", format!("{method} parameters are invalid"));
    let value = normalize(method, raw.get()).map_err(|_| invalid())?;
    if method == "install.execute" {
        let p: ExecuteParams = serde_json::from_value(value).map_err(|_| invalid())?;
        if !p.prepare.selection.valid_target() {
            return Err(invalid());
        }
        if p.prepare.selection.id.is_empty()
            || p.prepare.directory.is_empty()
            || p.store_directory.is_empty()
            || p.workers == 0
            || p.workers > 32
            || p.retries > 5
        {
            return Err(RpcError::new(
                "INVALID_PARAMS",
                "install.execute parameters are out of range",
            ));
        }
        if !matches!(p.java_policy.as_str(), "auto" | "local" | "managed") {
            return Err(RpcError::new(
                "INVALID_PARAMS",
                "java_policy must be auto, local, or managed",
            ));
        }
        Ok(Request::Execute(p))
    } else {
        let p: PrepareParams = serde_json::from_value(value).map_err(|_| invalid())?;
        if !p.selection.valid_target() {
            return Err(invalid());
        }
        if p.selection.id.is_empty() || p.directory.is_empty() {
            return Err(RpcError::new(
                "INVALID_PARAMS",
                "install.prepare id and directory must not be empty",
            ));
        }
        Ok(Request::Prepare(p))
    }
}
#[derive(Serialize)]
struct Envelope<'a, T> {
    protocol: u8,
    id: &'a str,
    event: &'a str,
    source: Source,
    #[serde(flatten)]
    payload: T,
}
fn line(out: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")
}
fn failure(out: &mut impl Write, id: &str, error: RpcError) -> io::Result<()> {
    protocol::error(out, Some(id), &error.code, &error.message)
}
pub fn write_event(out: &mut impl Write, id: &str, source: Source, event: Event) -> io::Result<()> {
    #[derive(Serialize)]
    struct FilePayload {
        file: FileEvent,
    }
    #[derive(Serialize)]
    struct RetryPayload {
        retry: RetryEvent,
    }
    #[derive(Serialize)]
    struct ProgressPayload {
        progress: Progress,
    }
    match event {
        Event::File(file) => line(
            out,
            &Envelope {
                protocol: 1,
                id,
                event: "file",
                source,
                payload: FilePayload { file },
            },
        )?,
        Event::Retry(retry) => line(
            out,
            &Envelope {
                protocol: 1,
                id,
                event: "retry",
                source,
                payload: RetryPayload { retry },
            },
        )?,
        Event::Progress(progress) => line(
            out,
            &Envelope {
                protocol: 1,
                id,
                event: "progress",
                source,
                payload: ProgressPayload { progress },
            },
        )?,
    }
    out.flush()
}
pub(crate) fn planning_error(e: metadata::Error) -> RpcError {
    let (code, message) = match e.name() {
        "VersionNotFound" => (
            "VERSION_NOT_FOUND",
            "The requested version is not in the selected manifest",
        ),
        "MetadataHashMismatch" | "AssetIndexHashMismatch" => (
            "INTEGRITY_ERROR",
            "Downloaded metadata failed SHA-1 verification",
        ),
        "UnsuccessfulHttpStatus" => (
            "HTTP_STATUS_ERROR",
            "The selected download source returned an unsuccessful HTTP status",
        ),
        "UnsafePath" | "UnsafeArchivePath" | "InsecureDownloadUrl" => (
            "UNSAFE_METADATA",
            "Metadata contains an unsafe download destination",
        ),
        _ => ("INSTALL_PLAN_ERROR", "Failed to create the install plan"),
    };
    RpcError::new(code, message)
}
fn prepare_error(e: Error) -> RpcError {
    let (code, message) = match e.name() {
        "CacheFileInvalid" | "FileNotFound" => (
            "CACHE_INVALID",
            "A required downloaded file is missing or invalid",
        ),
        "UnsafePath"
        | "UnsafeArchivePath"
        | "UnsupportedArchiveEntryType"
        | "ArchiveEntryTypeMismatch" => (
            "UNSAFE_ARCHIVE",
            "A native archive contains an unsafe entry",
        ),
        "ConflictingNativeEntry" => (
            "PREPARE_CONFLICT",
            "Runtime preparation found conflicting output paths",
        ),
        _ => ("PREPARE_FAILED", "Failed to prepare runtime files"),
    };
    RpcError::new(code, message)
}
pub fn handle(
    out: &mut impl Write,
    id: &str,
    method: &str,
    params: Option<&RawValue>,
) -> Option<io::Result<()>> {
    if !matches!(method, "install.execute" | "install.prepare") {
        return None;
    }
    if method == "install.execute" && super::vanilla::selected(params) {
        let request = match super::vanilla::parse(params) {
            Ok(p) => p,
            Err(e) => return Some(failure(out, id, e)),
        };
        let metadata = match version::HttpTransport::new() {
            Ok(t) => t,
            Err(e) => return Some(failure(out, id, planning_error(e))),
        };
        let service = version::Service::new(metadata, ".jmclcore/metadata-cache");
        let transport = match HttpTransport::new() {
            Ok(t) => t,
            Err(_) => {
                return Some(protocol::error(
                    out,
                    Some(id),
                    "DOWNLOAD_FAILED",
                    "A planned file could not be downloaded and verified",
                ));
            }
        };
        return Some(super::vanilla::run(out, id, request, &service, &transport));
    }
    let request = match parse(method, params) {
        Ok(p) => p,
        Err(e) => return Some(failure(out, id, e)),
    };
    let metadata = match version::HttpTransport::new() {
        Ok(t) => t,
        Err(e) => return Some(failure(out, id, planning_error(e))),
    };
    let service = version::Service::new(metadata, ".jmclcore/metadata-cache");
    let transport = match HttpTransport::new() {
        Ok(t) => t,
        Err(_) => {
            return Some(protocol::error(
                out,
                Some(id),
                "DOWNLOAD_FAILED",
                "A planned file could not be downloaded and verified",
            ));
        }
    };
    Some(run(out, id, request, &service, &transport))
}
pub fn handle_with<P: Planner, T: Transport>(
    out: &mut impl Write,
    id: &str,
    method: &str,
    params: Option<&RawValue>,
    planner: &P,
    transport: &T,
) -> Option<io::Result<()>> {
    if !matches!(method, "install.execute" | "install.prepare") {
        return None;
    }
    Some(match parse(method, params) {
        Ok(p) => run(out, id, p, planner, transport),
        Err(e) => failure(out, id, e),
    })
}
fn run<P: Planner, T: Transport>(
    out: &mut impl Write,
    id: &str,
    request: Request,
    planner: &P,
    transport: &T,
) -> io::Result<()> {
    let base = match &request {
        Request::Execute(p) => &p.prepare,
        Request::Prepare(p) => p,
    };
    let selection = &base.selection;
    let target = match selection.target() {
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
    let plan = match planner.plan(selection, target.clone()) {
        Ok(p) => p,
        Err(e) => return failure(out, id, planning_error(e)),
    };
    if matches!(&request, Request::Execute(_))
        && matches!(plan.loader.as_str(), "forge" | "neoforge")
        && service::processor_loader(&plan, selection).is_none()
    {
        return protocol::error(
            out,
            Some(id),
            "PROCESSOR_FAILED",
            "The processor plan has no loader version",
        );
    }
    let root = match base.directory.path().and_then(ArtifactRoot::open) {
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
    match request {
        Request::Execute(p) => {
            let store = match p.store_directory.path().and_then(Store::open) {
                Ok(s) => s,
                Err(_) => {
                    return protocol::error(
                        out,
                        Some(id),
                        "STORE_ROOT_ERROR",
                        "Failed to open the artifact store",
                    );
                }
            };
            #[derive(Serialize)]
            struct Started<'a> {
                stage: &'static str,
                version_id: &'a str,
                files_total: usize,
                bytes_total: u64,
            }
            #[derive(Serialize)]
            struct Payload<T> {
                started: T,
            }
            line(
                out,
                &Envelope {
                    protocol: 1,
                    id,
                    event: "started",
                    source: plan.source,
                    payload: Payload {
                        started: Started {
                            stage: "download",
                            version_id: &plan.version_id,
                            files_total: plan.files.len(),
                            bytes_total: plan.total_download_size,
                        },
                    },
                },
            )?;
            out.flush()?;
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
                |event| write_event(out, id, plan.source, event),
            )?;
            let summary = match result {
                Ok(s) => s,
                Err(_) => {
                    return protocol::error(
                        out,
                        Some(id),
                        "DOWNLOAD_FAILED",
                        "A planned file could not be downloaded and verified",
                    );
                }
            };
            let patched = if let Some(loader) =
                service::processor_loader(&plan, &p.prepare.selection)
            {
                let environment = std::env::vars_os().collect();
                let java = match service::processor_java(
                    &plan,
                    &p.java_paths,
                    &p.java_policy,
                    &p.store_directory.path()?,
                    &environment,
                ) {
                    Ok(java) => java,
                    Err(error) => {
                        let (code, message) = match error {
                            "EnvironmentUnavailable" => (
                                "ENVIRONMENT_UNAVAILABLE",
                                "Process environment is unavailable",
                            ),
                            "JavaNotFound" => {
                                ("JAVA_NOT_FOUND", "No Java executable candidates were found")
                            }
                            "JavaDownloadFailed" => (
                                "JAVA_DOWNLOAD_FAILED",
                                "The managed Java runtime download failed",
                            ),
                            _ => (
                                "JAVA_INCOMPATIBLE",
                                "No Java runtime is available for the processor chain",
                            ),
                        };
                        return protocol::error(out, Some(id), code, message);
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
                            Some(&environment),
                            &mut |event| sink(event),
                        )
                    },
                    |event| write_processor_event(out, id, plan.source, event),
                )?;
                match result {
                    Ok(summary) => Some(summary),
                    Err(error) => {
                        let loader_name = if plan.loader == "forge" {
                            "Forge"
                        } else {
                            "NeoForge"
                        };
                        let (code, message) = match error.name() {
                            "NoCompatibleJava" => (
                                "JAVA_INCOMPATIBLE",
                                format!(
                                    "No Java runtime is available for the {loader_name} processor chain"
                                ),
                            ),
                            "EnvironmentUnavailable" => (
                                "ENVIRONMENT_UNAVAILABLE",
                                "Process environment is unavailable".into(),
                            ),
                            _ => (
                                "PROCESSOR_FAILED",
                                format!("The {loader_name} processor chain failed"),
                            ),
                        };
                        return protocol::error(out, Some(id), code, &message);
                    }
                }
            } else {
                None
            };
            if service::record_completed(&root, &p.prepare.selection, &target).is_err() {
                return protocol::error(
                    out,
                    Some(id),
                    "INSTALL_STATE_ERROR",
                    "Failed to record the completed installation",
                );
            }
            #[derive(Serialize)]
            struct ResultPayload<'a> {
                stage: &'static str,
                version_id: &'a str,
                #[serde(flatten)]
                summary: Summary,
                #[serde(skip_serializing_if = "Option::is_none")]
                processors_run: Option<usize>,
                #[serde(skip_serializing_if = "Option::is_none")]
                patched_client: Option<&'a str>,
            }
            #[derive(Serialize)]
            struct PayloadResult<T> {
                result: T,
            }
            line(
                out,
                &Envelope {
                    protocol: 1,
                    id,
                    event: "result",
                    source: p.prepare.selection.source,
                    payload: PayloadResult {
                        result: ResultPayload {
                            stage: "download",
                            version_id: &p.prepare.selection.id,
                            summary,
                            processors_run: patched.as_ref().map(|s| s.processors_run),
                            patched_client: patched.as_ref().map(|s| s.patched_sha1.as_str()),
                        },
                    },
                },
            )
        }
        Request::Prepare(p) => {
            let result = match prepare(&root, &plan) {
                Ok(r) => r,
                Err(e) => return failure(out, id, prepare_error(e)),
            };
            #[derive(Serialize)]
            struct ResultPayload<'a> {
                stage: &'static str,
                version_id: &'a str,
                #[serde(flatten)]
                prepared: Preparation,
            }
            #[derive(Serialize)]
            struct Payload<T> {
                result: T,
            }
            line(
                out,
                &Envelope {
                    protocol: 1,
                    id,
                    event: "result",
                    source: p.selection.source,
                    payload: Payload {
                        result: ResultPayload {
                            stage: "prepare",
                            version_id: &p.selection.id,
                            prepared: result,
                        },
                    },
                },
            )
        }
    }
}

fn write_processor_event(
    out: &mut impl Write,
    id: &str,
    source: Source,
    event: crate::loaders::ProcessorStarted,
) -> io::Result<()> {
    #[derive(Serialize)]
    struct Started {
        stage: &'static str,
        #[serde(flatten)]
        event: crate::loaders::ProcessorStarted,
    }
    #[derive(Serialize)]
    struct Payload {
        processor_started: Started,
    }
    line(
        out,
        &Envelope {
            protocol: 1,
            id,
            event: "processor_started",
            source,
            payload: Payload {
                processor_started: Started {
                    stage: "processor",
                    event,
                },
            },
        },
    )?;
    out.flush()
}

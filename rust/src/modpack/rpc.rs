use super::{Error, provider, service};
use crate::install;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::io::{self, Write};
struct Secret(zeroize::Zeroizing<String>);
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(|s| Self(zeroize::Zeroizing::new(s)))
    }
}
fn store() -> String {
    ".jmclcore".into()
}
fn workers() -> u8 {
    8
}
fn retries() -> u8 {
    2
}
#[derive(Deserialize)]
struct Params {
    directory: String,
    id: String,
    file: String,
    api_key: Option<Secret>,
    #[serde(default = "store")]
    store_directory: String,
    #[serde(default = "workers", deserialize_with = "super::schema::u8_number")]
    workers: u8,
    #[serde(default = "retries", deserialize_with = "super::schema::u8_number")]
    retries: u8,
}
pub(crate) fn mapped(e: &Error) -> (&'static str, &'static str) {
    match e.name() {
        "InstanceAlreadyExists" => ("INSTANCE_EXISTS", "An instance with this id already exists"),
        "UnknownModpackFormat" => (
            "UNKNOWN_MODPACK_FORMAT",
            "The file is not a recognized modpack archive",
        ),
        "UnsupportedModpackFormat" => (
            "UNSUPPORTED_MODPACK_FORMAT",
            "The modpack format version is not supported",
        ),
        "UnsupportedModpackLoader" => (
            "UNSUPPORTED_LOADER",
            "The modpack's mod loader is not supported",
        ),
        "ModpackLoaderConflict" => (
            "LOADER_CONFLICT",
            "The modpack declares conflicting mod loaders",
        ),
        "CurseforgeApiKeyRequired" => (
            "INVALID_PARAMS",
            "CurseForge modpacks require an api_key supplied by the caller",
        ),
        "CurseforgeDownloadUnavailable" => (
            "DOWNLOAD_UNAVAILABLE",
            "A project file is not available for third-party downloads",
        ),
        "UnsupportedProjectType" => (
            "UNSUPPORTED_PROJECT_TYPE",
            "A project in the pack is not a mod, resource pack, or shader pack",
        ),
        "FileNotFound" => ("MODPACK_NOT_FOUND", "The modpack file does not exist"),
        "UnsafePath" => ("UNSAFE_MODPACK", "The modpack contains unsafe paths"),
        "UnsuccessfulHttpStatus" => (
            "HTTP_STATUS_ERROR",
            "The content API returned an unsuccessful HTTP status",
        ),
        s if s.starts_with("InstancesRootError:") => {
            ("INSTANCES_ROOT_ERROR", "Failed to open the instances root")
        }
        s if s.starts_with("StoreRootError:") => {
            ("STORE_ROOT_ERROR", "Failed to open the artifact store")
        }
        _ => ("MODPACK_INSTALL_FAILED", "Failed to install the modpack"),
    }
}
#[derive(Serialize)]
struct Event<'a, T> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    source: &'static str,
    #[serde(flatten)]
    payload: T,
}
#[derive(Serialize)]
struct FilePayload {
    file: install::FileEvent,
}
#[derive(Serialize)]
struct RetryPayload {
    retry: install::RetryEvent,
}
#[derive(Serialize)]
struct ProgressPayload {
    progress: install::Progress,
}
#[derive(Serialize)]
struct ResultEvent<'a> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    result: service::Summary,
}
fn line(out: &mut impl Write, v: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *out, v)?;
    out.write_all(b"\n")?;
    out.flush()
}
pub fn write_event(out: &mut impl Write, id: &str, event: install::Event) -> io::Result<()> {
    match event {
        install::Event::File(file) => line(
            out,
            &Event {
                protocol: 1,
                id,
                event: "file",
                source: "official",
                payload: FilePayload { file },
            },
        ),
        install::Event::Retry(retry) => line(
            out,
            &Event {
                protocol: 1,
                id,
                event: "retry",
                source: "official",
                payload: RetryPayload { retry },
            },
        ),
        install::Event::Progress(progress) => line(
            out,
            &Event {
                protocol: 1,
                id,
                event: "progress",
                source: "official",
                payload: ProgressPayload { progress },
            },
        ),
    }
}
pub(crate) use crate::install::stream::stream;
pub fn handle(
    method: &str,
    params: Option<&RawValue>,
    id: &str,
    out: &mut impl Write,
) -> Option<io::Result<()>> {
    if method != "modpack.install" {
        return None;
    }
    Some(inner(params, id, out))
}
fn inner(params: Option<&RawValue>, id: &str, out: &mut impl Write) -> io::Result<()> {
    let bad = |out: &mut _, message: &str| {
        crate::protocol::error(out, Some(id), "INVALID_PARAMS", message)
    };
    let Some(params) = params.filter(|p| p.get().trim() != "null") else {
        return bad(out, "modpack install requires directory, id, and file");
    };
    let p: Params = match serde_json::from_str(params.get()) {
        Ok(p) => p,
        Err(_) => return bad(out, "modpack install parameters are invalid"),
    };
    if p.directory.is_empty()
        || p.id.is_empty()
        || p.file.is_empty()
        || p.workers == 0
        || p.workers > 32
        || p.retries > 5
    {
        return bad(out, "modpack install parameters are out of range");
    }
    if crate::instances::validate_id(&p.id).is_err() {
        return bad(out, "the instance id is invalid");
    }
    if p.api_key
        .as_ref()
        .is_some_and(|s| provider::validate_key(&s.0).is_err())
    {
        return bad(out, "the curseforge api_key is invalid");
    }
    let result = stream(
        |sink| {
            let api = provider::HttpApi::new()?;
            let mut provider = provider::Provider {
                transport: api,
                key: p.api_key.as_ref().map_or("", |s| s.0.as_str()),
            };
            let transport = install::HttpTransport::new()?;
            service::install(
                service::Request {
                    directory: &p.directory,
                    store_directory: &p.store_directory,
                    id: &p.id,
                    file: &p.file,
                    api_key: p.api_key.as_ref().map(|s| s.0.as_str()),
                },
                install::Options {
                    workers: p.workers,
                    retries: p.retries,
                    sync_files: true,
                },
                &mut provider,
                &transport,
                Some(sink),
            )
        },
        |event| write_event(out, id, event),
    )?;
    match result {
        Ok(result) => line(
            out,
            &ResultEvent {
                protocol: 1,
                id,
                event: "result",
                result,
            },
        ),
        Err(e) => {
            let (code, message) = mapped(&e);
            crate::protocol::error(out, Some(id), code, message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn event() -> install::Event {
        install::Event::File(install::FileEvent {
            path: "fixture".into(),
            status: install::FileStatus::Cached,
            placement: "hard_link",
            size: 1,
            attempts: 0,
        })
    }
    #[test]
    fn concurrent_failed_writer_acknowledges_every_inflight_worker() {
        let finished = AtomicUsize::new(0);
        let mut writes = 0;
        let result = stream(
            |sink| {
                std::thread::scope(|scope| {
                    for _ in 0..8 {
                        let finished = &finished;
                        scope.spawn(move || {
                            for _ in 0..4 {
                                assert_eq!(sink(event()).unwrap_err().name(), "WriteFailed");
                            }
                            finished.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                });
                Err::<service::Summary, Error>("WriteFailed".into())
            },
            |_| {
                writes += 1;
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture"))
            },
        );
        assert!(result.is_err());
        assert_eq!(writes, 1);
        assert_eq!(finished.load(Ordering::SeqCst), 8);
    }
    #[test]
    fn writer_panic_drops_receiver_and_joins_worker() {
        let finished = AtomicUsize::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = stream(
                |sink| {
                    assert_eq!(sink(event()).unwrap_err().name(), "WriteFailed");
                    finished.fetch_add(1, Ordering::SeqCst);
                    Err::<service::Summary, Error>("WriteFailed".into())
                },
                |_| panic!("fixture writer"),
            );
        }));
        assert!(result.is_err());
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn service_panic_closes_channel_without_deadlock() {
        let result: io::Result<()> = stream(
            |_: &(dyn Fn(install::Event) -> Result<(), install::Error> + Sync)| {
                panic!("fixture worker")
            },
            |_| Ok(()),
        );
        assert!(result.is_err())
    }
}

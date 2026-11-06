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
    file: install::ByteFileEvent,
}
#[derive(Serialize)]
struct RetryPayload {
    retry: install::ByteRetryEvent,
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
    let event = match event {
        install::Event::File(file) => install::ByteEvent::File(install::ByteFileEvent {
            path: file.path.into(),
            status: file.status,
            placement: file.placement,
            size: file.size,
            attempts: file.attempts,
        }),
        install::Event::Retry(retry) => install::ByteEvent::Retry(install::ByteRetryEvent {
            path: retry.path.into(),
            attempt: retry.attempt,
            max_attempts: retry.max_attempts,
            error_name: retry.error_name,
        }),
        install::Event::Progress(progress) => install::ByteEvent::Progress(progress),
    };
    write_byte_event(out, id, event)
}
pub(crate) fn write_byte_event(
    out: &mut impl Write,
    id: &str,
    event: install::ByteEvent,
) -> io::Result<()> {
    match event {
        install::ByteEvent::File(file) => line(
            out,
            &Event {
                protocol: 1,
                id,
                event: "file",
                source: "official",
                payload: FilePayload { file },
            },
        ),
        install::ByteEvent::Retry(retry) => line(
            out,
            &Event {
                protocol: 1,
                id,
                event: "retry",
                source: "official",
                payload: RetryPayload { retry },
            },
        ),
        install::ByteEvent::Progress(progress) => line(
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
            service::install_bytes(
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
        |event| write_byte_event(out, id, event),
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

#[cfg(test)]
mod byte_event_tests {
    use super::*;

    #[test]
    fn file_and_retry_paths_preserve_utf8_or_opaque_bytes() {
        for path in [
            b"mods/plain.jar".to_vec(),
            "mods/世界.jar".as_bytes().to_vec(),
            vec![b'm', 255, 0, 254],
        ] {
            let expected = match std::str::from_utf8(&path) {
                Ok(text) => serde_json::json!(text),
                Err(_) => serde_json::json!(path),
            };
            let events = [
                (
                    "file",
                    install::ByteEvent::File(install::ByteFileEvent {
                        path: path.clone().into(),
                        status: install::FileStatus::Cached,
                        placement: "hard_link",
                        size: 9,
                        attempts: 0,
                    }),
                ),
                (
                    "retry",
                    install::ByteEvent::Retry(install::ByteRetryEvent {
                        path: path.into(),
                        attempt: 1,
                        max_attempts: 3,
                        error_name: "fixture".into(),
                    }),
                ),
            ];
            for (kind, event) in events {
                let mut out = Vec::new();
                write_byte_event(&mut out, "request", event).unwrap();
                assert_eq!(out.last(), Some(&b'\n'));
                let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
                assert_eq!(value["protocol"], 1);
                assert_eq!(value["id"], "request");
                assert_eq!(value["event"], kind);
                assert_eq!(value["source"], "official");
                assert_eq!(value[kind]["path"], expected);
                if kind == "retry" {
                    assert_eq!(value[kind]["error"], "fixture");
                    assert_eq!(value[kind]["attempt"], 1);
                    assert_eq!(value[kind]["max_attempts"], 3);
                }
            }
        }
    }

    #[test]
    fn public_string_writer_retains_wire_shape() {
        let mut out = Vec::new();
        write_event(
            &mut out,
            "request",
            install::Event::Retry(install::RetryEvent {
                path: "mods/世界.jar".into(),
                attempt: 1,
                max_attempts: 3,
                error_name: "fixture".into(),
            }),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&out).unwrap(),
            serde_json::json!({
                "protocol": 1, "id": "request", "event": "retry", "source": "official",
                "retry": {"path": "mods/世界.jar", "attempt": 1, "max_attempts": 3, "error": "fixture"}
            })
        );
    }
}

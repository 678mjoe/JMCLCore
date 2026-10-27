//! Shared vanilla completion ordering with separate RPC/CLI presentation.
use super::*;
use crate::{
    install,
    store::{ArtifactRoot, Store},
};
use std::{
    io::{self, Write},
    path::Path,
};
#[derive(Debug)]
pub(crate) enum Failure {
    Root(io::Error),
    Store(io::Error),
    Files(install::Error),
    Receipt(install::Error),
    Output(io::Error),
}
#[derive(Clone, Copy)]
pub(crate) struct CompletionInfo<'a> {
    pub version: &'a [u8],
    pub source: Source,
    pub files: usize,
    pub bytes: u64,
}
pub(crate) trait Presentation {
    fn started(&mut self, info: CompletionInfo<'_>) -> io::Result<()>;
    fn event(&mut self, event: install::ByteEvent) -> io::Result<()>;
    fn completed(&mut self, info: CompletionInfo<'_>, summary: &install::Summary)
    -> io::Result<()>;
}
fn event(
    out: &mut impl Write,
    id: &str,
    source: Source,
    kind: &str,
    payload: impl Serialize,
) -> io::Result<()> {
    // Same envelope field order as install::rpc; byte fields serialize through PlanBytes/BytePath.
    #[derive(Serialize)]
    struct Envelope<'a, T> {
        protocol: u8,
        id: &'a str,
        event: &'a str,
        source: Source,
        #[serde(flatten)]
        payload: T,
    }
    serde_json::to_writer(
        &mut *out,
        &Envelope {
            protocol: 1,
            id,
            event: kind,
            source,
            payload,
        },
    )?;
    out.write_all(b"\n")?;
    if kind == "result" {
        Ok(())
    } else {
        out.flush()
    }
}
struct Rpc<'a, W> {
    id: &'a str,
    source: Source,
    out: &'a mut W,
}
impl<W: Write> Presentation for Rpc<'_, W> {
    fn started(&mut self, info: CompletionInfo<'_>) -> io::Result<()> {
        #[derive(Serialize)]
        struct Started {
            stage: &'static str,
            version_id: install::BytePath,
            files_total: usize,
            bytes_total: u64,
        }
        #[derive(Serialize)]
        struct Payload<T> {
            started: T,
        }
        event(
            self.out,
            self.id,
            self.source,
            "started",
            Payload {
                started: Started {
                    stage: "download",
                    version_id: install::BytePath(info.version.to_vec()),
                    files_total: info.files,
                    bytes_total: info.bytes,
                },
            },
        )
    }
    fn event(&mut self, e: install::ByteEvent) -> io::Result<()> {
        #[derive(Serialize)]
        struct File<T> {
            file: T,
        }
        #[derive(Serialize)]
        struct Retry<T> {
            retry: T,
        }
        #[derive(Serialize)]
        struct Progress<T> {
            progress: T,
        }
        match e {
            install::ByteEvent::File(v) => {
                event(self.out, self.id, self.source, "file", File { file: v })
            }
            install::ByteEvent::Retry(v) => {
                event(self.out, self.id, self.source, "retry", Retry { retry: v })
            }
            install::ByteEvent::Progress(v) => event(
                self.out,
                self.id,
                self.source,
                "progress",
                Progress { progress: v },
            ),
        }
    }
    fn completed(
        &mut self,
        info: CompletionInfo<'_>,
        summary: &install::Summary,
    ) -> io::Result<()> {
        #[derive(Serialize)]
        struct ResultPayload<'a> {
            stage: &'static str,
            version_id: install::BytePath,
            #[serde(flatten)]
            summary: &'a install::Summary,
        }
        #[derive(Serialize)]
        struct Payload<T> {
            result: T,
        }
        event(
            self.out,
            self.id,
            self.source,
            "result",
            Payload {
                result: ResultPayload {
                    stage: "download",
                    version_id: install::BytePath(info.version.to_vec()),
                    summary,
                },
            },
        )
    }
}
enum FileInput<'a> {
    Ordinary(&'a OrdinaryFiles),
    Vanilla(&'a [install::ByteFile], u64),
}
impl ReadPlan {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "isolated ordinary controller is exercised by bounded fixtures"
        )
    )]
    pub(crate) fn complete_ordinary<T: install::Transport>(
        &self,
        id: &str,
        destination: &Path,
        cas: &Path,
        transport: &T,
        out: &mut impl Write,
    ) -> Result<Option<install::Summary>, Failure> {
        let Some(files) = self.ordinary_files() else {
            return Ok(None);
        };
        self.complete_files(
            FileInput::Ordinary(&files),
            || Ok(destination.to_path_buf()),
            || Ok(cas.to_path_buf()),
            install::Options {
                workers: 1,
                retries: 0,
                sync_files: true,
            },
            transport,
            &mut Rpc {
                id,
                source: self.source,
                out,
            },
        )
        .map(Some)
    }
    pub(crate) fn vanilla_download_phase(&self) -> Option<VanillaDownloadPhase<'_>> {
        (self.loader == "none").then_some(VanillaDownloadPhase { plan: self })
    }
    fn complete_files<T: install::Transport>(
        &self,
        files: FileInput<'_>,
        destination: impl FnOnce() -> io::Result<std::path::PathBuf>,
        cas: impl FnOnce() -> io::Result<std::path::PathBuf>,
        options: install::Options,
        transport: &T,
        presentation: &mut impl Presentation,
    ) -> Result<install::Summary, Failure> {
        let destination = destination().map_err(Failure::Root)?;
        if destination.as_os_str().is_empty() {
            return Err(Failure::Root(io::Error::new(
                io::ErrorKind::InvalidInput,
                "InvalidRoot",
            )));
        }
        let root = ArtifactRoot::open(destination).map_err(Failure::Root)?;
        let cas = cas().map_err(Failure::Store)?;
        if cas.as_os_str().is_empty() {
            return Err(Failure::Store(io::Error::new(
                io::ErrorKind::InvalidInput,
                "InvalidRoot",
            )));
        }
        let store = Store::open(cas).map_err(Failure::Store)?;
        let info = CompletionInfo {
            version: &self.version_id.0,
            source: self.source,
            files: self.files.len(),
            bytes: self.total_download_size,
        };
        presentation.started(info).map_err(Failure::Output)?;
        let summary = install::stream::stream(
            |sink| match files {
                FileInput::Ordinary(files) => {
                    files.execute(&root, &store, options, transport, Some(sink))
                }
                FileInput::Vanilla(files, total) => install::execute_bytes(
                    &root,
                    &store,
                    files,
                    total,
                    options,
                    transport,
                    Some(sink),
                ),
            },
            |event| presentation.event(event),
        )
        .map_err(Failure::Output)?
        .map_err(Failure::Files)?;
        install::receipt::write(
            &root,
            &install::receipt::Identity {
                version: &self.version_id.0,
                fabric: None,
                neo: None,
                forge: None,
                source: self.source,
            },
            &self.target,
        )
        .map_err(Failure::Receipt)?;
        presentation
            .completed(info, &summary)
            .map_err(Failure::Output)?;
        Ok(summary)
    }
}
/// Full plan remains borrowed; native/projection descriptions remain deferred.
pub(crate) struct VanillaDownloadPhase<'a> {
    plan: &'a ReadPlan,
}
impl VanillaDownloadPhase<'_> {
    pub(crate) fn complete<T: install::Transport>(
        &self,
        id: &str,
        destination: &[u8],
        cas: &[u8],
        options: install::Options,
        transport: &T,
        out: &mut impl Write,
    ) -> Result<install::Summary, Failure> {
        self.complete_with(
            destination,
            cas,
            options,
            transport,
            &mut Rpc {
                id,
                source: self.plan.source,
                out,
            },
        )
    }
    pub(crate) fn complete_with<T: install::Transport>(
        &self,
        destination: &[u8],
        cas: &[u8],
        options: install::Options,
        transport: &T,
        presentation: &mut impl Presentation,
    ) -> Result<install::Summary, Failure> {
        let files: Vec<_> = self
            .plan
            .files
            .iter()
            .map(|f| install::ByteFile {
                path: install::BytePath(f.path.0.clone()),
                url: f.url.clone(),
                fallback_url: f.fallback_url.clone(),
                sha1: f.sha1.clone(),
                size: f.size,
            })
            .collect();
        self.plan.complete_files(
            FileInput::Vanilla(&files, self.plan.total_download_size),
            || crate::store::path_from_bytes(destination).map(Into::into),
            || crate::store::path_from_bytes(cas).map(Into::into),
            options,
            transport,
            presentation,
        )
    }
}
#[cfg(all(test, target_os = "linux"))]
#[path = "plan_completion_tests.rs"]
mod tests;

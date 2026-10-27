//! Shared vanilla download completion; preparation descriptions remain deferred.
use super::*;
use crate::{
    install,
    store::{ArtifactRoot, Store},
};
use std::{
    io::{self, Write},
    path::Path,
};

#[allow(
    dead_code,
    reason = "private unrouted controller errors are consumed by bounded fixtures"
)]
#[derive(Debug)]
pub(crate) enum Failure {
    Root,
    Store,
    Files(install::Error),
    Receipt(install::Error),
    Output(io::Error),
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
enum FileInput<'a> {
    Ordinary(&'a OrdinaryFiles),
    Vanilla(&'a [install::ByteFile], u64),
}
impl ReadPlan {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "private completion seam is not RPC routed")
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
            id,
            FileInput::Ordinary(&files),
            || Ok(destination.to_path_buf()),
            || Ok(cas.to_path_buf()),
            install::Options {
                workers: 1,
                retries: 0,
                sync_files: true,
            },
            transport,
            out,
        )
        .map(Some)
    }
    pub(crate) fn vanilla_download_phase(&self) -> Option<VanillaDownloadPhase<'_>> {
        (self.loader == "none").then_some(VanillaDownloadPhase { plan: self })
    }
    #[allow(clippy::too_many_arguments)]
    fn complete_files<T: install::Transport>(
        &self,
        id: &str,
        files: FileInput<'_>,
        destination: impl FnOnce() -> io::Result<std::path::PathBuf>,
        cas: impl FnOnce() -> io::Result<std::path::PathBuf>,
        options: install::Options,
        transport: &T,
        out: &mut impl Write,
    ) -> Result<install::Summary, Failure> {
        let destination = destination().map_err(|_| Failure::Root)?;
        if destination.as_os_str().is_empty() {
            return Err(Failure::Root);
        }
        let root = ArtifactRoot::open(destination).map_err(|_| Failure::Root)?;
        let cas = cas().map_err(|_| Failure::Store)?;
        if cas.as_os_str().is_empty() {
            return Err(Failure::Store);
        }
        let store = Store::open(cas).map_err(|_| Failure::Store)?;
        #[derive(Serialize)]
        struct Started<'a> {
            stage: &'static str,
            version_id: &'a PlanBytes,
            files_total: usize,
            bytes_total: u64,
        }
        #[derive(Serialize)]
        struct StartedPayload<T> {
            started: T,
        }
        #[derive(Serialize)]
        struct FilePayload<T> {
            file: T,
        }
        #[derive(Serialize)]
        struct RetryPayload<T> {
            retry: T,
        }
        #[derive(Serialize)]
        struct ProgressPayload<T> {
            progress: T,
        }
        event(
            out,
            id,
            self.source,
            "started",
            StartedPayload {
                started: Started {
                    stage: "download",
                    version_id: &self.version_id,
                    files_total: self.files.len(),
                    bytes_total: self.total_download_size,
                },
            },
        )
        .map_err(Failure::Output)?;
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
            |e| match e {
                install::ByteEvent::File(v) => {
                    event(out, id, self.source, "file", FilePayload { file: v })
                }
                install::ByteEvent::Retry(v) => {
                    event(out, id, self.source, "retry", RetryPayload { retry: v })
                }
                install::ByteEvent::Progress(v) => event(
                    out,
                    id,
                    self.source,
                    "progress",
                    ProgressPayload { progress: v },
                ),
            },
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
        #[derive(Serialize)]
        struct ResultPayload<'a> {
            stage: &'static str,
            version_id: &'a PlanBytes,
            #[serde(flatten)]
            summary: &'a install::Summary,
        }
        #[derive(Serialize)]
        struct Payload<T> {
            result: T,
        }
        event(
            out,
            id,
            self.source,
            "result",
            Payload {
                result: ResultPayload {
                    stage: "download",
                    version_id: &self.version_id,
                    summary: &summary,
                },
            },
        )
        .map_err(Failure::Output)?;
        Ok(summary)
    }
}
/// Download-phase capability: keeps the entire validated plan alive. Native and
/// projection descriptions belong to later preparation, never to this phase.
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
            id,
            FileInput::Vanilla(&files, self.plan.total_download_size),
            || crate::store::path_from_bytes(destination).map(Into::into),
            || crate::store::path_from_bytes(cas).map(Into::into),
            options,
            transport,
            out,
        )
    }
}
#[cfg(all(test, target_os = "linux"))]
#[path = "plan_completion_tests.rs"]
mod tests;

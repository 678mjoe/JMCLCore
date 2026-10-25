//! Exact launch event envelopes. Only redacted argv and redacted log bytes enter this writer.
use super::{
    Plan,
    process::{Event, RecordEnd, Stream},
};
use crate::metadata::Source;
use base64::Engine;
use serde::Serialize;
use std::io::{self, Write};
#[derive(Serialize)]
struct Started<'a> {
    stage: &'static str,
    version_id: &'a str,
    pid: u64,
    redacted_argv: Vec<&'a str>,
}
#[derive(Serialize)]
struct StartedEnvelope<'a> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    source: Source,
    started: Started<'a>,
}
#[derive(Serialize)]
struct Log<'a> {
    pub(crate) sequence: u64,
    encoding: &'static str,
    end: RecordEnd,
    data: &'a str,
}
#[derive(Serialize)]
struct StdoutEnvelope<'a> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    source: Source,
    stdout: Log<'a>,
}
#[derive(Serialize)]
struct StderrEnvelope<'a> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    source: Source,
    stderr: Log<'a>,
}
pub struct EventWriter<'a, W: Write> {
    pub writer: &'a mut W,
    pub request_id: &'a str,
    pub source: Source,
    pub version_id: &'a str,
    pub plan: &'a Plan,
    pub(crate) sequence: u64,
    pub(crate) failed: bool,
}
impl<'a, W: Write> EventWriter<'a, W> {
    pub fn new(
        writer: &'a mut W,
        request_id: &'a str,
        source: Source,
        version_id: &'a str,
        plan: &'a Plan,
    ) -> Self {
        Self {
            writer,
            request_id,
            source,
            version_id,
            plan,
            sequence: 0,
            failed: false,
        }
    }
    pub fn output_failed(&self) -> bool {
        self.failed
    }
    pub fn emit(&mut self, event: Event<'_>) {
        if self.failed {
            return;
        }
        if self
            .write(event)
            .and_then(|()| self.writer.flush())
            .is_err()
        {
            self.failed = true
        }
    }
    fn write(&mut self, event: Event<'_>) -> io::Result<()> {
        match event {
            Event::Started { pid } => serde_json::to_writer(
                &mut *self.writer,
                &StartedEnvelope {
                    protocol: 1,
                    id: self.request_id,
                    event: "started",
                    source: self.source,
                    started: Started {
                        stage: "launch",
                        version_id: self.version_id,
                        pid,
                        redacted_argv: self.plan.redacted_argv().collect(),
                    },
                },
            )?,
            Event::Log { stream, bytes, end } => {
                self.sequence += 1;
                let mut encoded = [0u8; 21848];
                let n = base64::engine::general_purpose::STANDARD
                    .encode_slice(bytes, &mut encoded)
                    .map_err(io::Error::other)?;
                let data = std::str::from_utf8(&encoded[..n]).map_err(io::Error::other)?;
                let log = Log {
                    sequence: self.sequence,
                    encoding: "base64",
                    end,
                    data,
                };
                match stream {
                    Stream::Stdout => serde_json::to_writer(
                        &mut *self.writer,
                        &StdoutEnvelope {
                            protocol: 1,
                            id: self.request_id,
                            event: "stdout",
                            source: self.source,
                            stdout: log,
                        },
                    )?,
                    Stream::Stderr => serde_json::to_writer(
                        &mut *self.writer,
                        &StderrEnvelope {
                            protocol: 1,
                            id: self.request_id,
                            event: "stderr",
                            source: self.source,
                            stderr: log,
                        },
                    )?,
                }
            }
        }
        self.writer.write_all(b"\n")
    }
}

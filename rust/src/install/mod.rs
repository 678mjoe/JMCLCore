//! Verified download executor. Network bytes are staged inside the CAS and only
//! published after exact size/hash verification; every instance gets a hard link
//! whenever its filesystem supports it. This module does not launch a game.
use crate::{
    plan::{Plan, validate_relative_path},
    store::{ArtifactRoot, Placement, Store},
};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::{
    io::{self, Read, Write},
    sync::Mutex,
    time::{Duration, Instant},
};

pub const PROGRESS_BYTE_INTERVAL: u64 = 256 * 1024;
pub const PROGRESS_TIME_INTERVAL: Duration = Duration::from_millis(100);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl Error {
    pub fn named(name: &str) -> Self {
        Self(name.into())
    }
    pub fn name(&self) -> &str {
        &self.0
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        // Pinned Zig 0.16 POSIX create/open paths classify EILSEQ as
        // BadPathName (observed for non-UTF8 filenames on APFS).
        #[cfg(unix)]
        if e.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error()) {
            return Self::named("BadPathName");
        }
        Self(
            match e.kind() {
                io::ErrorKind::NotFound => "FileNotFound",
                io::ErrorKind::PermissionDenied => "AccessDenied",
                io::ErrorKind::AlreadyExists => "PathAlreadyExists",
                io::ErrorKind::InvalidInput => return Self(e.to_string()),
                io::ErrorKind::InvalidData => return Self(e.to_string()),
                io::ErrorKind::TimedOut => "ConnectionTimedOut",
                io::ErrorKind::Interrupted => "Canceled",
                _ => "FileIoError",
            }
            .into(),
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub workers: u8,
    pub retries: u8,
    pub sync_files: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            workers: 8,
            retries: 2,
            sync_files: true,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub files_total: usize,
    pub files_downloaded: usize,
    pub files_cached: usize,
    pub files_hard_linked: usize,
    pub files_copied: usize,
    pub bytes_verified: u64,
    pub bytes_transferred: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub files_completed: usize,
    pub files_total: usize,
    pub bytes_verified: u64,
    pub bytes_processed: u64,
    pub bytes_total: u64,
    pub bytes_transferred: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Cached,
    Downloaded,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileEvent {
    pub path: String,
    pub status: FileStatus,
    pub placement: &'static str,
    pub size: u64,
    pub attempts: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RetryEvent {
    pub path: String,
    pub attempt: u8,
    pub max_attempts: u8,
    #[serde(rename = "error")]
    pub error_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    File(FileEvent),
    Retry(RetryEvent),
    Progress(Progress),
}
/// Internal content path carrier. URLs and public version/install plans remain
/// Strings; only filenames use the source string-or-byte-array representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BytePath(pub Vec<u8>);
impl Serialize for BytePath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => serializer.serialize_str(text),
            Err(_) => self.0.serialize(serializer),
        }
    }
}
impl From<Vec<u8>> for BytePath {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}
impl From<String> for BytePath {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}
impl From<&str> for BytePath {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}
impl AsRef<[u8]> for BytePath {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
#[derive(Clone, Debug)]
pub(crate) struct ByteFile {
    pub path: BytePath,
    pub url: String,
    pub fallback_url: Option<String>,
    pub sha1: String,
    pub size: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ByteFileEvent {
    pub path: BytePath,
    pub status: FileStatus,
    pub placement: &'static str,
    pub size: u64,
    pub attempts: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ByteRetryEvent {
    pub path: BytePath,
    pub attempt: u8,
    pub max_attempts: u8,
    #[serde(rename = "error")]
    pub error_name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ByteEvent {
    File(ByteFileEvent),
    Retry(ByteRetryEvent),
    Progress(Progress),
}
// Only the public String-plan adapter calls this. Its paths originate as &str;
// conversion is nevertheless checked rather than lossy or unchecked.
fn string_event(event: ByteEvent) -> Result<Event, Error> {
    let text =
        |path: BytePath| String::from_utf8(path.0).map_err(|_| Error::named("InvalidUtf8Path"));
    Ok(match event {
        ByteEvent::File(file) => Event::File(FileEvent {
            path: text(file.path)?,
            status: file.status,
            placement: file.placement,
            size: file.size,
            attempts: file.attempts,
        }),
        ByteEvent::Retry(retry) => Event::Retry(RetryEvent {
            path: text(retry.path)?,
            attempt: retry.attempt,
            max_attempts: retry.max_attempts,
            error_name: retry.error_name,
        }),
        ByteEvent::Progress(progress) => Event::Progress(progress),
    })
}
#[derive(Clone, Copy)]
enum ExecutionPath<'a> {
    String(&'a str),
    Bytes(&'a [u8]),
}
impl ExecutionPath<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::String(s) => s.as_bytes(),
            Self::Bytes(b) => b,
        }
    }
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::String(s) => validate_relative_path(s).map_err(|e| Error::named(e.name())),
            Self::Bytes(bytes) => validate_content_path(bytes),
        }
    }
    fn verify(&self, root: &ArtifactRoot, hash: &str, size: u64) -> io::Result<bool> {
        match self {
            Self::String(s) => root.verify(s, hash, size),
            Self::Bytes(b) => root.verify_bytes(b, hash, size),
        }
    }
    fn import(
        &self,
        tx: &crate::store::ObjectTransaction<'_>,
        root: &ArtifactRoot,
        size: u64,
    ) -> io::Result<Placement> {
        match self {
            Self::String(s) => tx.import_from(root, s, size),
            Self::Bytes(b) => tx.import_bytes(root, b, size),
        }
    }
    fn materialize(
        &self,
        tx: &crate::store::ObjectTransaction<'_>,
        root: &ArtifactRoot,
        size: u64,
    ) -> io::Result<Placement> {
        match self {
            Self::String(s) => tx.materialize_at(root, s, size),
            Self::Bytes(b) => tx.materialize_bytes(root, b, size),
        }
    }
}
fn validate_content_path(path: &[u8]) -> Result<(), Error> {
    // Byte-for-byte counterpart of plan::validate_relative_path/components;
    // not the weaker native/archive path validator.
    for component in path.split(|b| *b == b'/') {
        if component.is_empty()
            || component == b"."
            || component == b".."
            || component.ends_with(b".")
            || component.ends_with(b" ")
            || component
                .iter()
                .any(|b| *b < 32 || *b == 127 || matches!(b, b'/' | b'\\' | b':'))
        {
            return Err(Error::named("UnsafePath"));
        }
        let stem = component
            .split(|b| *b == b'.')
            .next()
            .unwrap()
            .to_ascii_uppercase();
        if matches!(stem.as_slice(), b"CON" | b"PRN" | b"AUX" | b"NUL")
            || (stem.len() == 4
                && (stem.starts_with(b"COM") || stem.starts_with(b"LPT"))
                && matches!(stem[3], b'1'..=b'9'))
        {
            return Err(Error::named("UnsafePath"));
        }
    }
    Ok(())
}
struct ExecutionFile<'a> {
    path: ExecutionPath<'a>,
    url: &'a str,
    fallback_url: Option<&'a str>,
    sha1: &'a str,
    size: u64,
}
/// A response must stream into the supplied bounded sink. HTTP status is returned
/// after the body so its verification/error precedence matches the Zig executor.
pub trait Transport: Sync {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, Error>;
}
pub struct HttpTransport(reqwest::blocking::Client);
impl HttpTransport {
    pub fn new() -> Result<Self, Error> {
        reqwest::blocking::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("jmcl-core/0.1.0-dev")
            .build()
            .map(Self)
            .map_err(|_| Error::named("TlsInitializationFailed"))
    }
}
impl Transport for HttpTransport {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, Error> {
        let mut r = self.0.get(url).send().map_err(|e| {
            Error::named(if e.is_timeout() {
                "ConnectionTimedOut"
            } else if e.is_redirect() {
                "TooManyHttpRedirects"
            } else {
                "HttpConnectionFailed"
            })
        })?;
        let status = r.status().as_u16();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = r
                .read(&mut buf)
                .map_err(|_| Error::named("HttpReadFailed"))?;
            if n == 0 {
                break;
            }
            sink.write_all(&buf[..n])
                .map_err(|_| Error::named("WriteFailed"))?;
        }
        Ok(status)
    }
}
struct State {
    next: usize,
    error: Option<Error>,
    summary: Summary,
    progress: Progress,
    file_progress: Vec<u64>,
    last_stream: Option<Instant>,
}
struct Shared<'a> {
    state: Mutex<State>,
    reporter: Option<&'a (dyn Fn(ByteEvent) -> Result<(), Error> + Sync)>,
}
impl Shared<'_> {
    fn emit(&self, event: ByteEvent) -> Result<(), Error> {
        if let Some(r) = self.reporter {
            r(event)?;
        }
        Ok(())
    }
    fn next(&self, total: usize) -> Option<usize> {
        let mut s = self.state.lock().unwrap();
        if s.error.is_some() || s.next == total {
            None
        } else {
            let i = s.next;
            s.next += 1;
            Some(i)
        }
    }
    fn fail(&self, e: Error) {
        let mut s = self.state.lock().unwrap();
        if s.error.is_none() {
            s.error = Some(e)
        }
    }
    fn bytes(&self, index: usize, bytes: u64) -> Result<(), Error> {
        let mut s = self.state.lock().unwrap();
        if bytes <= s.file_progress[index] {
            return Ok(());
        }
        s.progress.bytes_processed += bytes - s.file_progress[index];
        s.file_progress[index] = bytes;
        let now = Instant::now();
        if s.last_stream
            .is_some_and(|last| now.duration_since(last) < PROGRESS_TIME_INTERVAL)
        {
            return Ok(());
        }
        s.last_stream = Some(now);
        self.emit(ByteEvent::Progress(s.progress.clone()))
    }
    fn complete(
        &self,
        index: usize,
        file: &ExecutionFile<'_>,
        downloaded: bool,
        placement: Placement,
        attempts: u8,
    ) -> Result<(), Error> {
        let mut s = self.state.lock().unwrap();
        s.progress.bytes_processed += file.size.saturating_sub(s.file_progress[index]);
        s.file_progress[index] = file.size;
        if downloaded {
            s.summary.files_downloaded += 1;
            s.summary.bytes_transferred += file.size;
            s.progress.bytes_transferred += file.size;
        } else {
            s.summary.files_cached += 1;
        }
        match placement {
            Placement::HardLink => s.summary.files_hard_linked += 1,
            Placement::Copy => s.summary.files_copied += 1,
        };
        s.summary.bytes_verified += file.size;
        s.progress.files_completed += 1;
        s.progress.bytes_verified += file.size;
        s.last_stream = Some(Instant::now());
        self.emit(ByteEvent::File(ByteFileEvent {
            path: BytePath(file.path.bytes().to_vec()),
            status: if downloaded {
                FileStatus::Downloaded
            } else {
                FileStatus::Cached
            },
            placement: match placement {
                Placement::HardLink => "hard_link",
                Placement::Copy => "copy",
            },
            size: file.size,
            attempts,
        }))?;
        self.emit(ByteEvent::Progress(s.progress.clone()))
    }
}
struct LimitedSink<'a, D: Digest> {
    output: &'a mut std::fs::File,
    expected: u64,
    received: u64,
    last_reported: u64,
    hash: D,
    on_bytes: &'a mut dyn FnMut(u64) -> Result<(), Error>,
    failure: Option<Error>,
}
impl<D: Digest> Write for LimitedSink<'_, D> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let allowed = (self.expected - self.received).min(bytes.len() as u64) as usize;
        if allowed == 0 && !bytes.is_empty() {
            self.failure = Some(Error::named("ResponseTooLarge"));
            return Err(io::Error::other("ResponseTooLarge"));
        }
        let n = match self.output.write(&bytes[..allowed]) {
            Ok(n) => n,
            Err(e) => {
                self.failure = Some(Error::from(e));
                return Err(io::Error::other("FileWriteFailed"));
            }
        };
        self.hash.update(&bytes[..n]);
        self.received += n as u64;
        if self.received - self.last_reported >= PROGRESS_BYTE_INTERVAL {
            self.last_reported = self.received;
            if let Err(e) = (self.on_bytes)(self.received) {
                self.failure = Some(e);
                return Err(io::Error::other("ReporterFailed"));
            }
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
/// Stream a bounded download into a caller-owned, newly created empty staging
/// file and verify its exact size and digest. `D` may be `sha1::Sha1` or any
/// compatible `digest::Digest` implementation (for example `sha2::Sha256`).
/// This does not publish or sync the file; on failure the caller must discard it.
/// Bytes callbacks report this attempt's absolute count every 256 KiB. Returning
/// an error from the callback interrupts the transfer and preserves that error.
pub fn download_into_staging_file<D: Digest, T: Transport>(
    transport: &T,
    url: &str,
    expected_digest: &str,
    size: u64,
    output: &mut std::fs::File,
    on_bytes: &mut dyn FnMut(u64) -> Result<(), Error>,
) -> Result<(), Error> {
    if expected_digest.len() != <D as Digest>::output_size() * 2
        || !expected_digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::named("InvalidDigest"));
    }
    download_bounded_into_staging_file::<D, T>(
        transport,
        url,
        DownloadExpectation {
            digest: expected_digest,
            size,
            exact_size: true,
        },
        output,
        on_bytes,
    )
    .map(|_| ())
}
/// Runtime catalogs may advertise only a maximum archive size. Digest validation
/// still occurs after fetching, including malformed catalog digest strings.
pub struct DownloadExpectation<'a> {
    pub digest: &'a str,
    pub size: u64,
    pub exact_size: bool,
}
pub fn download_bounded_into_staging_file<D: Digest, T: Transport>(
    transport: &T,
    url: &str,
    expected: DownloadExpectation<'_>,
    output: &mut std::fs::File,
    on_bytes: &mut dyn FnMut(u64) -> Result<(), Error>,
) -> Result<u64, Error> {
    use std::io::Seek;
    if output.metadata()?.len() != 0 || output.stream_position()? != 0 {
        return Err(Error::named("InvalidStagingFile"));
    }
    let mut sink = LimitedSink {
        output,
        expected: expected.size,
        received: 0,
        last_reported: 0,
        hash: D::new(),
        on_bytes,
        failure: None,
    };
    let response = transport.fetch(url, &mut sink);
    if let Some(e) = sink.failure.take() {
        return Err(e);
    }
    let status = response?;
    sink.flush()?;
    if !(200..300).contains(&status) {
        return Err(Error::named("UnsuccessfulHttpStatus"));
    }
    if expected.exact_size && sink.received != expected.size {
        return Err(Error::named("DownloadSizeMismatch"));
    }
    let actual: String = sink
        .hash
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected.digest) {
        return Err(Error::named("DownloadHashMismatch"));
    }
    Ok(sink.received)
}

/// Executes planned immutable files. Plan paths and digests are validated before
/// creating any worker or file. No outer caller lock may wrap this function.
pub fn execute<T: Transport>(
    destination: &ArtifactRoot,
    store: &Store,
    plan: &Plan,
    options: Options,
    transport: &T,
    reporter: Option<&(dyn Fn(Event) -> Result<(), Error> + Sync)>,
) -> Result<Summary, Error> {
    let files: Vec<_> = plan
        .files
        .iter()
        .map(|f| ExecutionFile {
            path: ExecutionPath::String(&f.path),
            url: &f.url,
            fallback_url: f.fallback_url.as_deref(),
            sha1: &f.sha1,
            size: f.size,
        })
        .collect();
    let adapter = |event| {
        if let Some(reporter) = reporter {
            reporter(string_event(event)?)
        } else {
            Ok(())
        }
    };
    execute_files(
        destination,
        store,
        &files,
        plan.total_download_size,
        options,
        transport,
        Some(&adapter),
    )
}
/// Additive content-only execution view; no native/archive projection or Java
/// path is entered. Uses the same scheduler, bounded sink and digest transaction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_bytes<T: Transport>(
    destination: &ArtifactRoot,
    store: &Store,
    files: &[ByteFile],
    total: u64,
    options: Options,
    transport: &T,
    reporter: Option<&(dyn Fn(ByteEvent) -> Result<(), Error> + Sync)>,
) -> Result<Summary, Error> {
    let files: Vec<_> = files
        .iter()
        .map(|f| ExecutionFile {
            path: ExecutionPath::Bytes(&f.path.0),
            url: &f.url,
            fallback_url: f.fallback_url.as_deref(),
            sha1: &f.sha1,
            size: f.size,
        })
        .collect();
    execute_files(
        destination,
        store,
        &files,
        total,
        options,
        transport,
        reporter,
    )
}
#[allow(clippy::too_many_arguments)]
fn execute_files<T: Transport>(
    destination: &ArtifactRoot,
    store: &Store,
    files: &[ExecutionFile<'_>],
    expected_total: u64,
    options: Options,
    transport: &T,
    reporter: Option<&(dyn Fn(ByteEvent) -> Result<(), Error> + Sync)>,
) -> Result<Summary, Error> {
    if options.workers == 0 || options.workers > 32 {
        return Err(Error::named("InvalidWorkerCount"));
    }
    if options.retries > 5 {
        return Err(Error::named("InvalidRetryCount"));
    }
    let mut total = 0u64;
    let mut paths = std::collections::HashSet::new();
    for f in files {
        f.path.validate()?;
        crate::store::object_path(f.sha1)?;
        if !paths.insert(f.path.bytes()) {
            return Err(Error::named("ConflictingDestination"));
        }
        total = total
            .checked_add(f.size)
            .ok_or_else(|| Error::named("DownloadSizeOverflow"))?;
    }
    if total != expected_total {
        return Err(Error::named("InvalidPlanTotal"));
    }
    let shared = Shared {
        state: Mutex::new(State {
            next: 0,
            error: None,
            summary: Summary {
                files_total: files.len(),
                ..Default::default()
            },
            progress: Progress {
                files_total: files.len(),
                bytes_total: total,
                ..Default::default()
            },
            file_progress: vec![0; files.len()],
            last_stream: None,
        }),
        reporter,
    };
    std::thread::scope(|scope| {
        for _ in 0..usize::from(options.workers).min(files.len()) {
            let shared = &shared;
            scope.spawn(move || {
                while let Some(index) = shared.next(files.len()) {
                    if let Err(e) = one(
                        destination,
                        store,
                        &files[index],
                        index,
                        options,
                        transport,
                        shared,
                    ) {
                        shared.fail(e);
                        break;
                    }
                }
            });
        }
    });
    let state = shared.state.into_inner().unwrap();
    match state.error {
        Some(e) => Err(e),
        None => Ok(state.summary),
    }
}
fn one<T: Transport>(
    destination: &ArtifactRoot,
    store: &Store,
    file: &ExecutionFile<'_>,
    index: usize,
    options: Options,
    transport: &T,
    shared: &Shared<'_>,
) -> Result<(), Error> {
    let destination_valid = file.path.verify(destination, file.sha1, file.size)?;
    let transaction = store.transaction(file.sha1)?;
    let mut downloaded = false;
    let mut attempts = 0;
    if !transaction.verify(file.size)? {
        transaction.remove_invalid(file.size)?;
        if destination_valid {
            file.path.import(&transaction, destination, file.size)?;
        } else {
            let max = options.retries + 1;
            for attempt in 1..=max {
                let url = if max > 1 && attempt == max {
                    file.fallback_url.unwrap_or(file.url)
                } else {
                    file.url
                };
                let result: Result<(), Error> = (|| {
                    let mut staged = transaction.stage(file.size)?;
                    download_into_staging_file::<Sha1, _>(
                        transport,
                        url,
                        file.sha1,
                        file.size,
                        staged.file_mut(),
                        &mut |received| shared.bytes(index, received),
                    )?;
                    staged.publish(options.sync_files)?;
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        downloaded = true;
                        attempts = attempt;
                        break;
                    }
                    Err(e) if attempt < max => {
                        shared.emit(ByteEvent::Retry(ByteRetryEvent {
                            path: BytePath(file.path.bytes().to_vec()),
                            attempt,
                            max_attempts: max,
                            error_name: e.0,
                        }))?;
                        std::thread::sleep(Duration::from_millis(200 * u64::from(attempt)));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
    let placement = file
        .path
        .materialize(&transaction, destination, file.size)?;
    shared.complete(index, file, downloaded, placement, attempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        metadata::Source,
        plan::{File as PlannedFile, Target, Usage},
    };
    use std::sync::{
        Arc, Condvar,
        atomic::{AtomicUsize, Ordering},
    };
    fn file(path: &str, bytes: &[u8]) -> PlannedFile {
        PlannedFile {
            path: path.into(),
            url: "https://fixture.invalid/primary".into(),
            fallback_url: Some("https://fixture.invalid/fallback".into()),
            sha1: format!("{:x}", Sha1::digest(bytes)),
            size: bytes.len() as u64,
            usage: Usage {
                library: true,
                ..Default::default()
            },
        }
    }
    fn plan(files: Vec<PlannedFile>) -> Plan {
        Plan {
            total_download_size: files.iter().map(|f| f.size).sum(),
            files,
            version_id: "test".into(),
            source: Source::Official,
            target: Target::host().unwrap(),
            classpath: vec![],
            native_extractions: vec![],
            asset_projections: vec![],
            java_major_version: 17,
            loader: "none".into(),
        }
    }
    struct Fake {
        bodies: Mutex<Vec<Vec<u8>>>,
        requests: Mutex<Vec<String>>,
        status: u16,
    }
    impl Fake {
        fn new(bodies: Vec<Vec<u8>>) -> Self {
            Self {
                bodies: Mutex::new(bodies),
                requests: Mutex::new(vec![]),
                status: 200,
            }
        }
    }
    impl Transport for Fake {
        fn fetch(&self, url: &str, out: &mut dyn Write) -> Result<u16, Error> {
            self.requests.lock().unwrap().push(url.into());
            let body = self.bodies.lock().unwrap().remove(0);
            for chunk in body.chunks(64 * 1024) {
                out.write_all(chunk)
                    .map_err(|_| Error::named("WriteFailed"))?;
            }
            Ok(self.status)
        }
    }
    #[cfg(unix)]
    #[test]
    fn path_encoding_errno_matches_zig_without_normalizing_other_errors() {
        assert_eq!(
            Error::from(io::Error::from_raw_os_error(
                rustix::io::Errno::ILSEQ.raw_os_error()
            ))
            .name(),
            "BadPathName"
        );
        let unrelated = io::Error::from_raw_os_error(rustix::io::Errno::INVAL.raw_os_error());
        let unchanged_text = unrelated.to_string();
        assert_eq!(Error::from(unrelated).name(), unchanged_text);
        assert_eq!(
            Error::from(io::Error::from_raw_os_error(
                rustix::io::Errno::IO.raw_os_error()
            ))
            .name(),
            "FileIoError"
        );
        assert_eq!(
            Error::from(io::Error::new(
                io::ErrorKind::InvalidInput,
                "fixture path error"
            ))
            .name(),
            "fixture path error"
        );
        assert_eq!(
            Error::from(io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture data error"
            ))
            .name(),
            "fixture data error"
        );
    }
    #[test]
    fn maximum_size_helper_accepts_short_verified_and_checks_bad_digest_after_fetch() {
        let transport = Fake::new(vec![b"abc".to_vec(), b"abc".to_vec()]);
        let hash = format!("{:x}", Sha1::digest(b"abc"));
        let mut output = tempfile::tempfile().unwrap();
        assert_eq!(
            download_bounded_into_staging_file::<Sha1, _>(
                &transport,
                "https://test",
                DownloadExpectation {
                    digest: &hash,
                    size: 100,
                    exact_size: false
                },
                &mut output,
                &mut |_| Ok(())
            )
            .unwrap(),
            3
        );
        let mut output = tempfile::tempfile().unwrap();
        assert_eq!(
            download_bounded_into_staging_file::<Sha1, _>(
                &transport,
                "https://test",
                DownloadExpectation {
                    digest: &"z".repeat(40),
                    size: 100,
                    exact_size: false
                },
                &mut output,
                &mut |_| Ok(())
            )
            .unwrap_err()
            .name(),
            "DownloadHashMismatch"
        );
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
    }
    #[test]
    fn staging_helper_rejects_nonempty_and_preserves_interrupt() {
        let mut output = tempfile::tempfile().unwrap();
        output.write_all(b"existing").unwrap();
        let transport = Fake::new(vec![]);
        let hash = format!("{:x}", Sha1::digest(b""));
        assert_eq!(
            download_into_staging_file::<Sha1, _>(
                &transport,
                "https://unused",
                &hash,
                0,
                &mut output,
                &mut |_| Ok(())
            )
            .unwrap_err()
            .name(),
            "InvalidStagingFile"
        );
        assert!(transport.requests.lock().unwrap().is_empty());
        let bytes = vec![7; PROGRESS_BYTE_INTERVAL as usize + 1];
        let hash = format!("{:x}", Sha1::digest(&bytes));
        let transport = Fake::new(vec![bytes.clone()]);
        let mut output = tempfile::tempfile().unwrap();
        assert_eq!(
            download_into_staging_file::<Sha1, _>(
                &transport,
                "https://unused",
                &hash,
                bytes.len() as u64,
                &mut output,
                &mut |_| Err(Error::named("Interrupted"))
            )
            .unwrap_err()
            .name(),
            "Interrupted"
        );
        assert_eq!(output.metadata().unwrap().len(), PROGRESS_BYTE_INTERVAL);
    }
    #[test]
    fn verifies_stream_publishes_hardlinks_and_reuses_without_network() {
        let temp = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(temp.path().join("first")).unwrap();
        let store = Store::open(temp.path().join("store")).unwrap();
        let p = plan(vec![file("libraries/a.jar", b"abc")]);
        let fake = Fake::new(vec![b"abc".to_vec()]);
        let events = Mutex::new(vec![]);
        let reporter = |e| {
            events.lock().unwrap().push(e);
            Ok(())
        };
        let result = execute(
            &root,
            &store,
            &p,
            Options::default(),
            &fake,
            Some(&reporter),
        )
        .unwrap();
        assert_eq!(
            result,
            Summary {
                files_total: 1,
                files_downloaded: 1,
                files_cached: 0,
                files_hard_linked: 1,
                files_copied: 0,
                bytes_verified: 3,
                bytes_transferred: 3
            }
        );
        assert_eq!(events.lock().unwrap().len(), 2);
        let second = ArtifactRoot::open(temp.path().join("second")).unwrap();
        let next = execute(&second, &store, &p, Options::default(), &fake, None).unwrap();
        assert_eq!(next.files_cached, 1);
        assert_eq!(fake.requests.lock().unwrap().len(), 1);
        assert_eq!(
            std::fs::read(temp.path().join("second/libraries/a.jar")).unwrap(),
            b"abc"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let a = std::fs::metadata(temp.path().join("first/libraries/a.jar")).unwrap();
            let b = std::fs::metadata(temp.path().join("second/libraries/a.jar")).unwrap();
            assert_eq!(a.ino(), b.ino());
            assert!(a.nlink() >= 3);
        }
    }
    #[test]
    fn imports_valid_destination_and_never_fetches() {
        let t = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(t.path().join("game")).unwrap();
        std::fs::write(t.path().join("game/a.jar"), b"abc").unwrap();
        let store = Store::open(t.path().join("store")).unwrap();
        let fake = Fake::new(vec![]);
        let summary = execute(
            &root,
            &store,
            &plan(vec![file("a.jar", b"abc")]),
            Options::default(),
            &fake,
            None,
        )
        .unwrap();
        assert_eq!(summary.files_cached, 1);
        assert_eq!(summary.bytes_transferred, 0);
    }
    #[test]
    fn size_hash_status_failures_publish_nothing_and_release_lock() {
        for (body, status, name) in [
            (b"ab".to_vec(), 200, "DownloadSizeMismatch"),
            (b"abcd".to_vec(), 200, "ResponseTooLarge"),
            (b"bad".to_vec(), 200, "DownloadHashMismatch"),
            (b"abc".to_vec(), 503, "UnsuccessfulHttpStatus"),
        ] {
            let t = tempfile::tempdir().unwrap();
            let root = ArtifactRoot::open(t.path().join("game")).unwrap();
            let store = Store::open(t.path().join("store")).unwrap();
            let mut fake = Fake::new(vec![body, b"abc".to_vec()]);
            fake.status = status;
            let p = plan(vec![file("a.jar", b"abc")]);
            let options = Options {
                retries: 0,
                ..Default::default()
            };
            assert_eq!(
                execute(&root, &store, &p, options, &fake, None)
                    .unwrap_err()
                    .name(),
                name
            );
            assert!(!root.verify("a.jar", &p.files[0].sha1, 3).unwrap());
            assert!(!store.verify(&p.files[0].sha1, 3).unwrap());
            let prefix = t
                .path()
                .join("store/objects/sha1")
                .join(&p.files[0].sha1[..2]);
            assert_eq!(std::fs::read_dir(prefix).unwrap().count(), 0);
            fake.status = 200;
            assert!(execute(&root, &store, &p, options, &fake, None).is_ok());
        }
    }
    #[test]
    fn retries_use_final_fallback_and_monotonic_processed_bytes() {
        let t = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(t.path().join("game")).unwrap();
        let store = Store::open(t.path().join("store")).unwrap();
        let bytes = vec![42; 600_000];
        let mut wrong = bytes.clone();
        wrong[0] = 0;
        let fake = Fake::new(vec![wrong, bytes.clone()]);
        let events = Mutex::new(vec![]);
        let reporter = |e| {
            events.lock().unwrap().push(e);
            Ok(())
        };
        let p = plan(vec![file("large.jar", &bytes)]);
        let summary = execute(
            &root,
            &store,
            &p,
            Options {
                workers: 1,
                retries: 1,
                sync_files: true,
            },
            &fake,
            Some(&reporter),
        )
        .unwrap();
        assert_eq!(
            *fake.requests.lock().unwrap(),
            vec![
                p.files[0].url.clone(),
                p.files[0].fallback_url.clone().unwrap()
            ]
        );
        assert_eq!(summary.bytes_transferred, 600_000);
        let mut last = 0;
        let events = events.lock().unwrap();
        assert!(events.iter().any(|e|matches!(e,Event::Retry(r) if r.attempt==1&&r.max_attempts==2&&r.error_name=="DownloadHashMismatch")));
        for e in &*events {
            if let Event::Progress(p) = e {
                assert!(p.bytes_processed >= last && p.bytes_processed <= 600_000);
                last = p.bytes_processed;
            }
        }
        assert_eq!(last, 600_000);
        assert!(
            events.iter().any(
                |e| matches!(e,Event::Progress(p) if p.files_completed==0&&p.bytes_processed>0)
            )
        );
    }
    #[test]
    fn duplicate_digest_workers_download_once() {
        let t = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(t.path().join("game")).unwrap();
        let store = Store::open(t.path().join("store")).unwrap();
        let fake = Fake::new(vec![b"abc".to_vec()]);
        let p = plan(vec![file("a.jar", b"abc"), file("b.jar", b"abc")]);
        let summary = execute(
            &root,
            &store,
            &p,
            Options {
                workers: 2,
                ..Default::default()
            },
            &fake,
            None,
        )
        .unwrap();
        assert_eq!(summary.files_downloaded, 1);
        assert_eq!(summary.files_cached, 1);
        assert_eq!(summary.files_hard_linked, 2);
        assert_eq!(fake.requests.lock().unwrap().len(), 1);
    }
    #[test]
    fn two_installers_share_guard_and_failed_reporter_releases() {
        struct Gate {
            calls: AtomicUsize,
            state: Mutex<(bool, bool)>,
            changed: Condvar,
        }
        impl Transport for Gate {
            fn fetch(&self, _: &str, out: &mut dyn Write) -> Result<u16, Error> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let mut state = self.state.lock().unwrap();
                state.0 = true;
                self.changed.notify_all();
                while !state.1 {
                    state = self.changed.wait(state).unwrap();
                }
                drop(state);
                out.write_all(b"abc")?;
                Ok(200)
            }
        }
        let t = tempfile::tempdir().unwrap();
        let root1 = ArtifactRoot::open(t.path().join("one")).unwrap();
        let root2 = ArtifactRoot::open(t.path().join("two")).unwrap();
        let store = Store::open(t.path().join("store")).unwrap();
        let gate = Arc::new(Gate {
            calls: AtomicUsize::new(0),
            state: Mutex::new((false, false)),
            changed: Condvar::new(),
        });
        let p = plan(vec![file("a.jar", b"abc")]);
        std::thread::scope(|scope| {
            let a = scope.spawn(|| execute(&root1, &store, &p, Options::default(), &*gate, None));
            {
                let mut s = gate.state.lock().unwrap();
                while !s.0 {
                    s = gate.changed.wait(s).unwrap();
                }
            }
            let b = scope.spawn(|| execute(&root2, &store, &p, Options::default(), &*gate, None));
            {
                let mut s = gate.state.lock().unwrap();
                s.1 = true;
                gate.changed.notify_all();
            }
            assert!(a.join().unwrap().is_ok());
            assert!(b.join().unwrap().is_ok());
        });
        assert_eq!(gate.calls.load(Ordering::SeqCst), 1);
        let fail = |_| Err(Error::named("BrokenPipe"));
        assert_eq!(
            execute(&root1, &store, &p, Options::default(), &*gate, Some(&fail))
                .unwrap_err()
                .name(),
            "BrokenPipe"
        );
        assert!(execute(&root1, &store, &p, Options::default(), &*gate, None).is_ok());
    }
    #[test]
    fn validation_rejects_unsafe_paths_invalid_options_and_totals() {
        let t = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(t.path().join("g")).unwrap();
        let store = Store::open(t.path().join("s")).unwrap();
        let fake = Fake::new(vec![]);
        for path in ["../escape", "/absolute", "a\\b", "a//b"] {
            assert!(
                execute(
                    &root,
                    &store,
                    &plan(vec![file(path, b"abc")]),
                    Options::default(),
                    &fake,
                    None
                )
                .is_err()
            );
        }
        let mut p = plan(vec![]);
        assert_eq!(
            execute(
                &root,
                &store,
                &p,
                Options {
                    workers: 0,
                    ..Default::default()
                },
                &fake,
                None
            )
            .unwrap_err()
            .name(),
            "InvalidWorkerCount"
        );
        p.total_download_size = 1;
        assert_eq!(
            execute(&root, &store, &p, Options::default(), &fake, None)
                .unwrap_err()
                .name(),
            "InvalidPlanTotal"
        );
    }
    #[cfg(unix)]
    #[test]
    fn anchored_destination_rejects_symlink_descendant() {
        use std::os::unix::fs::symlink;
        let t = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(t.path().join("game")).unwrap();
        let store = Store::open(t.path().join("store")).unwrap();
        std::fs::create_dir(t.path().join("outside")).unwrap();
        symlink(t.path().join("outside"), t.path().join("game/libraries")).unwrap();
        let fake = Fake::new(vec![b"abc".to_vec()]);
        assert!(
            execute(
                &root,
                &store,
                &plan(vec![file("libraries/a.jar", b"abc")]),
                Options::default(),
                &fake,
                None
            )
            .is_err()
        );
        assert!(!t.path().join("outside/a.jar").exists());
    }
    #[test]
    fn transaction_blocks_gc_until_materialized() {
        use crate::store::GcOptions;
        let t = tempfile::tempdir().unwrap();
        let root = ArtifactRoot::open(t.path().join("game")).unwrap();
        let store = Store::open(t.path().join("store")).unwrap();
        let hash = format!("{:x}", Sha1::digest(b"abc"));
        {
            let tx = store.transaction(&hash).unwrap();
            let mut stage = tx.stage(3).unwrap();
            stage.file_mut().write_all(b"abc").unwrap();
            stage.publish(true).unwrap();
        }
        let tx = store.transaction(&hash).unwrap();
        std::thread::scope(|scope| {
            let (send, receive) = std::sync::mpsc::channel();
            let store_ref = &store;
            let gc = scope.spawn(move || {
                send.send(()).unwrap();
                store_ref
                    .collect(GcOptions {
                        dry_run: false,
                        min_age_seconds: 0,
                    })
                    .unwrap()
            });
            receive.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(
                tx.materialize_at(&root, "a.jar", 3).unwrap(),
                Placement::HardLink
            );
            drop(tx);
            let result = gc.join().unwrap();
            assert_eq!(result.objects_deleted, 0);
            assert_eq!(result.objects_linked, 1);
        });
        assert!(root.verify("a.jar", &hash, 3).unwrap());
    }
}

mod native_zip;
mod prepare;
pub use prepare::{Preparation, cleanup_native, prepare};

pub mod cli;
pub mod rpc;
pub mod service;
pub mod stream;

#[cfg(test)]
#[path = "content_byte_tests.rs"]
mod content_byte_tests;

pub(crate) mod receipt;

pub(crate) mod vanilla;

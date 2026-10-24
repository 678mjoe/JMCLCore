//! Literal-argv subprocess execution with bounded streaming and no reader threads.
use super::Error;
use serde::Serialize;
use std::{
    collections::HashMap,
    ffi::OsString,
    io::{self, Read},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecordEnd {
    Newline,
    Eof,
    More,
}
pub enum Event<'a> {
    Started {
        pid: u64,
    },
    Log {
        stream: Stream,
        bytes: &'a [u8],
        end: RecordEnd,
    },
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub pid: u64,
    pub termination: &'static str,
    pub exit_code: Option<u8>,
    pub signal: Option<u32>,
    pub status: Option<u32>,
    pub duration_ms: i64,
    pub max_rss: Option<usize>,
}
pub type Environment = HashMap<OsString, OsString>;
const ALLOWED: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "PATH",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "SystemRoot",
    "WINDIR",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "PULSE_SERVER",
    "__CF_USER_TEXT_ENCODING",
];
pub fn environment(parent: &Environment) -> Environment {
    ALLOWED
        .iter()
        .filter_map(|k| {
            #[cfg(not(windows))]
            let value = parent.get(std::ffi::OsStr::new(k));
            #[cfg(windows)]
            let value = parent
                .iter()
                .find(|(name, _)| name.to_str().is_some_and(|n| n.eq_ignore_ascii_case(k)))
                .map(|(_, v)| v);
            value.map(|v| (OsString::from(k), v.clone()))
        })
        .collect()
}

struct Framer {
    stream: Stream,
    bytes: Zeroizing<[u8; 16384]>,
    len: usize,
}
impl Framer {
    fn new(stream: Stream) -> Self {
        Self {
            stream,
            bytes: Zeroizing::new([0; 16384]),
            len: 0,
        }
    }
    fn emit(&mut self, end: RecordEnd, cb: &mut impl FnMut(Event<'_>)) {
        let len = if end != RecordEnd::More && self.len > 0 && self.bytes[self.len - 1] == b'\r' {
            self.len - 1
        } else {
            self.len
        };
        cb(Event::Log {
            stream: self.stream,
            bytes: &self.bytes[..len],
            end,
        });
        self.bytes[..self.len].zeroize();
        self.len = 0;
    }
    fn feed(&mut self, b: &[u8], cb: &mut impl FnMut(Event<'_>)) {
        for &c in b {
            if c == b'\n' {
                self.emit(RecordEnd::Newline, cb);
                continue;
            }
            if self.len == self.bytes.len() {
                self.emit(RecordEnd::More, cb)
            }
            self.bytes[self.len] = c;
            self.len += 1
        }
    }
    fn finish(&mut self, cb: &mut impl FnMut(Event<'_>)) {
        if self.len > 0 {
            self.emit(RecordEnd::Eof, cb)
        }
    }
}
struct Redactor<'a> {
    pending: Zeroizing<[u8; 32768]>,
    len: usize,
    secrets: &'a [&'a str],
    maximum: usize,
    framer: Framer,
}
impl<'a> Redactor<'a> {
    fn new(secrets: &'a [&'a str], stream: Stream) -> Self {
        Self {
            pending: Zeroizing::new([0; 32768]),
            len: 0,
            secrets,
            maximum: secrets.iter().map(|s| s.len()).max().unwrap_or(1).max(1),
            framer: Framer::new(stream),
        }
    }
    fn consume(&mut self, n: usize) {
        let remaining = self.len - n;
        self.pending.copy_within(n..self.len, 0);
        self.pending[remaining..self.len].zeroize();
        self.len = remaining;
    }
    fn process(&mut self, final_: bool, cb: &mut impl FnMut(Event<'_>)) {
        while self.len > 0 {
            let mut best = None;
            for s in self.secrets {
                if s.is_empty() {
                    continue;
                }
                if let Some(pos) = self.pending[..self.len]
                    .windows(s.len())
                    .position(|b| b == s.as_bytes())
                    && best.is_none_or(|(p, n)| pos < p || (pos == p && s.len() > n))
                {
                    best = Some((pos, s.len()))
                }
            }
            if let Some((pos, n)) = best {
                self.framer.feed(&self.pending[..pos], cb);
                self.framer.feed(b"<redacted>", cb);
                self.consume(pos + n)
            } else {
                let keep = if final_ {
                    0
                } else {
                    self.len.min(self.maximum - 1)
                };
                let n = self.len - keep;
                if n == 0 {
                    break;
                }
                self.framer.feed(&self.pending[..n], cb);
                self.consume(n)
            }
        }
    }
    fn feed(&mut self, b: &[u8], cb: &mut impl FnMut(Event<'_>)) {
        for chunk in b.chunks(4096) {
            self.pending[self.len..self.len + chunk.len()].copy_from_slice(chunk);
            self.len += chunk.len();
            self.process(false, cb)
        }
    }
    fn finish(&mut self, cb: &mut impl FnMut(Event<'_>)) {
        self.process(true, cb);
        self.framer.finish(cb)
    }
}
fn io_name(e: io::Error) -> Error {
    match e.kind() {
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        io::ErrorKind::InvalidInput => "InvalidArgument",
        _ => "Unexpected",
    }
}
#[cfg(unix)]
fn nonblocking<T: std::os::fd::AsFd>(p: &T) -> Result<(), Error> {
    use rustix::fs::*;
    let flags = fcntl_getfl(p).map_err(|_| "Unexpected")?;
    fcntl_setfl(p, flags | OFlags::NONBLOCK).map_err(|_| "Unexpected")
}
#[cfg(unix)]
fn drain<T: Read>(
    p: &mut T,
    r: &mut Redactor<'_>,
    cb: &mut impl FnMut(Event<'_>),
) -> Result<(), Error> {
    let mut bytes = Zeroizing::new([0; 4096]);
    for _ in 0..64 {
        match p.read(&mut *bytes) {
            Ok(0) => break,
            Ok(n) => r.feed(&bytes[..n], cb),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io_name(e)),
        }
    }
    Ok(())
}
/// Unix wait4 preserves the oracle's per-child resource usage (bytes, not KiB).
#[cfg(unix)]
pub fn run(
    argv: &[&str],
    cwd: &str,
    secrets: &[&str],
    parent: &Environment,
    mut cb: impl FnMut(Event<'_>),
) -> Result<ResultData, Error> {
    if argv.is_empty() {
        return Err("EmptyArgv");
    }
    if cwd.is_empty() {
        return Err("EmptyWorkingDirectory");
    }
    if secrets.iter().any(|s| s.len() > 16384) {
        return Err("SecretTooLong");
    }
    let mut child = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .envs(environment(parent))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_name)?;
    let pid = child.id();
    let mut reaped = false;
    let result = (|| {
        let mut out = child.stdout.take().ok_or("Unexpected")?;
        let mut err = child.stderr.take().ok_or("Unexpected")?;
        nonblocking(&out)?;
        nonblocking(&err)?;
        cb(Event::Started { pid: pid.into() });
        let start = Instant::now();
        let mut ro = Redactor::new(secrets, Stream::Stdout);
        let mut re = Redactor::new(secrets, Stream::Stderr);
        loop {
            drain(&mut out, &mut ro, &mut cb)?;
            drain(&mut err, &mut re, &mut cb)?;
            let mut status = 0;
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            let waited =
                unsafe { libc::wait4(pid as libc::pid_t, &mut status, libc::WNOHANG, &mut usage) };
            if waited < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(io_name(e));
            }
            if waited > 0 {
                reaped = true;
                drain(&mut out, &mut ro, &mut cb)?;
                drain(&mut err, &mut re, &mut cb)?;
                ro.finish(&mut cb);
                re.finish(&mut cb);
                let mut r = ResultData {
                    pid: pid.into(),
                    termination: "unknown",
                    exit_code: None,
                    signal: None,
                    status: None,
                    duration_ms: start.elapsed().as_millis().min(i64::MAX as u128) as i64,
                    max_rss: Some(usage.ru_maxrss as usize),
                };
                #[cfg(not(target_os = "macos"))]
                {
                    r.max_rss = r.max_rss.map(|n| n.saturating_mul(1024));
                }
                if libc::WIFEXITED(status) {
                    r.termination = "exited";
                    r.exit_code = Some(libc::WEXITSTATUS(status) as u8)
                } else if libc::WIFSIGNALED(status) {
                    r.termination = "signal";
                    r.signal = Some(libc::WTERMSIG(status) as u32)
                } else if libc::WIFSTOPPED(status) {
                    r.termination = "stopped";
                    r.signal = Some(libc::WSTOPSIG(status) as u32)
                } else {
                    r.status = Some(status as u32)
                }
                return Ok(r);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    if !reaped {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_and_split_redaction() {
        let mut r = Redactor::new(&["fake-secret"], Stream::Stdout);
        let mut logs = Vec::new();
        let mut cb = |e: Event<'_>| {
            if let Event::Log { bytes, end, .. } = e {
                logs.push((bytes.to_vec(), end))
            }
        };
        r.feed(b"prefix fake-", &mut cb);
        r.feed(b"secret\r\n\nlast\r", &mut cb);
        r.finish(&mut cb);
        assert_eq!(
            logs,
            vec![
                (b"prefix <redacted>".to_vec(), RecordEnd::Newline),
                (vec![], RecordEnd::Newline),
                (b"last".to_vec(), RecordEnd::Eof)
            ]
        )
    }
    #[test]
    fn exact_limit_newline() {
        let mut f = Framer::new(Stream::Stdout);
        let mut ends = vec![];
        let mut cb = |e: Event<'_>| {
            if let Event::Log { bytes, end, .. } = e {
                ends.push((bytes.len(), end))
            }
        };
        f.feed(&vec![b'x'; 16384], &mut cb);
        f.feed(b"\n", &mut cb);
        f.finish(&mut cb);
        assert_eq!(ends, vec![(16384, RecordEnd::Newline)])
    }
    #[test]
    fn secret_environment_filtered() {
        let env = HashMap::from([
            (OsString::from("HOME"), OsString::from("home")),
            (OsString::from("FAKE_TOKEN"), OsString::from("fake-secret")),
            (
                OsString::from("JAVA_TOOL_OPTIONS"),
                OsString::from("ignored"),
            ),
        ]);
        assert_eq!(environment(&env).len(), 1)
    }
}

#[cfg(windows)]
fn drain_windows<T: Read + std::os::windows::io::AsRawHandle>(
    pipe: &mut T,
    r: &mut Redactor<'_>,
    cb: &mut impl FnMut(Event<'_>),
) -> Result<(), Error> {
    use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, GetLastError};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            h: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    let mut bytes = Zeroizing::new([0; 4096]);
    for _ in 0..64 {
        let mut available = 0;
        let ok = unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            if unsafe { GetLastError() } == ERROR_BROKEN_PIPE {
                return Ok(());
            }
            return Err("Unexpected");
        };
        if available == 0 {
            return Ok(());
        }
        let n = pipe
            .read(&mut bytes[..(available as usize).min(4096)])
            .map_err(io_name)?;
        if n == 0 {
            break;
        }
        r.feed(&bytes[..n], cb)
    }
    Ok(())
}
#[cfg(windows)]
pub fn run(
    argv: &[&str],
    cwd: &str,
    secrets: &[&str],
    parent: &Environment,
    mut cb: impl FnMut(Event<'_>),
) -> Result<ResultData, Error> {
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_nonpaged: usize,
        quota_nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            h: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    if argv.is_empty() {
        return Err("EmptyArgv");
    }
    if cwd.is_empty() {
        return Err("EmptyWorkingDirectory");
    }
    if secrets.iter().any(|s| s.len() > 16384) {
        return Err("SecretTooLong");
    }
    let mut child = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .envs(environment(parent))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(io_name)?;
    let pid = u64::from(child.id());
    let result = (|| {
        let mut out = child.stdout.take().ok_or("Unexpected")?;
        let mut err = child.stderr.take().ok_or("Unexpected")?;
        cb(Event::Started { pid });
        let start = Instant::now();
        let mut ro = Redactor::new(secrets, Stream::Stdout);
        let mut re = Redactor::new(secrets, Stream::Stderr);
        loop {
            drain_windows(&mut out, &mut ro, &mut cb)?;
            drain_windows(&mut err, &mut re, &mut cb)?;
            if let Some(status) = child.try_wait().map_err(io_name)? {
                drain_windows(&mut out, &mut ro, &mut cb)?;
                drain_windows(&mut err, &mut re, &mut cb)?;
                ro.finish(&mut cb);
                re.finish(&mut cb);
                let mut counters: Counters = unsafe { std::mem::zeroed() };
                counters.cb = std::mem::size_of::<Counters>() as u32;
                let ok = unsafe {
                    GetProcessMemoryInfo(
                        child.as_raw_handle(),
                        &mut counters,
                        std::mem::size_of::<Counters>() as u32,
                    )
                };
                return Ok(ResultData {
                    pid,
                    termination: "exited",
                    exit_code: status.code().map(|n| n as u8),
                    signal: None,
                    status: None,
                    duration_ms: start.elapsed().as_millis().min(i64::MAX as u128) as i64,
                    max_rss: if ok != 0 {
                        Some(counters.peak_working_set)
                    } else {
                        None
                    },
                });
            }
            std::thread::sleep(Duration::from_millis(2))
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

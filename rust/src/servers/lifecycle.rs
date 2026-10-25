//! Linux server lifecycle, anchored to an already opened server directory.
//! PID alone is never authority: the JVM's random identity argument must match.
use rustix::fs::{self, AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::os::unix::{fs::MetadataExt, io::AsRawFd};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_LOG_READ_BYTES: usize = 64 * 1024;
pub const MAX_LOG_READ_BYTES: usize = 1024 * 1024;
pub const MAX_LOG_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_GRACE_MS: u64 = 120_000;
const STATE: &str = ".jmcl/server.pid";
const FIFO: &str = ".jmcl/console.fifo";
const LOG: &str = "logs/console.log";
pub type Result<T> = std::result::Result<T, &'static str>;
fn one() -> u32 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    #[serde(default = "one", deserialize_with = "crate::instances::schema_number")]
    pub schema_version: u32,
    #[serde(deserialize_with = "crate::instances::schema_number")]
    pub pid: u32,
    #[serde(deserialize_with = "crate::instances::schema_number")]
    pub supervisor_pid: u32,
    pub identity: String,
    pub java_path: String,
    #[serde(deserialize_with = "super::numbers::persisted_i64")]
    pub started_at_ms: i64,
}
impl State {
    fn valid(&self) -> bool {
        self.schema_version == 1
            && self.pid > 1
            && self.supervisor_pid > 1
            && self.identity.len() == 32
            && self.identity.bytes().all(|b| b.is_ascii_hexdigit())
            && !self.java_path.is_empty()
            && self.java_path.len() <= 4096
            && self.started_at_ms > 0
    }
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub running: bool,
    pub stale_state: bool,
    pub pid: Option<u32>,
    pub supervisor_pid: Option<u32>,
    pub java_path: Option<String>,
    pub started_at_ms: Option<i64>,
    pub uptime_ms: Option<i64>,
}
impl Status {
    fn stopped(stale_state: bool) -> Self {
        Self {
            running: false,
            stale_state,
            pid: None,
            supervisor_pid: None,
            java_path: None,
            started_at_ms: None,
            uptime_ms: None,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct StartResult {
    pub pid: u32,
    pub supervisor_pid: u32,
    pub started_at_ms: i64,
}
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    Graceful,
    Terminated,
    Killed,
    NotRunning,
}
#[derive(Debug, Serialize)]
pub struct StopResult {
    pub termination: Termination,
    pub pid: Option<u32>,
}
#[derive(Debug, Serialize)]
pub struct Logs {
    pub text: String,
    pub file_id: u64,
    pub start_cursor: u64,
    pub next_cursor: u64,
    pub eof: bool,
    pub reset: bool,
    pub truncated: bool,
}
pub(crate) fn open(dir: &File, path: &str, flags: OFlags, mode: u32) -> io::Result<File> {
    fs::openat(
        dir,
        path,
        flags | OFlags::CLOEXEC,
        Mode::from_raw_mode(mode),
    )
    .map(File::from)
    .map_err(Into::into)
}
pub(crate) fn mkdir(dir: &File, name: &str) -> io::Result<()> {
    match fs::mkdirat(dir, name, Mode::from_raw_mode(0o755)) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub(crate) fn read(dir: &File, path: &str, max: usize) -> io::Result<Vec<u8>> {
    let f = open(dir, path, OFlags::RDONLY | OFlags::NONBLOCK, 0)?;
    if !f.metadata()?.is_file() {
        return Err(io::Error::other("NotFile"));
    }
    let mut b = Vec::new();
    f.take(max as u64 + 1).read_to_end(&mut b)?;
    if b.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "TooLarge"));
    }
    Ok(b)
}
pub(crate) fn atomic_write(dir: &File, path: &str, bytes: &[u8]) -> io::Result<()> {
    let mut random = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let prefix = path
        .rsplit_once('/')
        .map(|(p, _)| format!("{p}/"))
        .unwrap_or_default();
    let tmp = format!("{prefix}.jmcl-state-{:032x}", u128::from_le_bytes(random));
    let mut f = open(
        dir,
        &tmp,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW,
        0o600,
    )?;
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::renameat(dir, tmp.as_str(), dir, path).map_err(Into::into)
    })();
    if result.is_err() {
        let _ = fs::unlinkat(dir, tmp.as_str(), AtFlags::empty());
    }
    result
}
pub struct StoppedGuard {
    _lock: File,
}
fn lock(dir: &File) -> io::Result<File> {
    mkdir(dir, ".jmcl")?;
    let f = open(
        dir,
        ".jmcl/lifecycle.lock",
        OFlags::RDWR | OFlags::CREATE,
        0o600,
    )?;
    fs::flock(&f, fs::FlockOperation::LockExclusive)?;
    Ok(f)
}
pub fn lock_stopped(dir: &File) -> Result<StoppedGuard> {
    let lock = lock(dir).map_err(|_| "ServerStatusFailed")?;
    if status_unlocked(dir)
        .map_err(|_| "ServerStatusFailed")?
        .running
    {
        return Err("ServerAlreadyRunning");
    }
    Ok(StoppedGuard { _lock: lock })
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
fn load_state(dir: &File) -> io::Result<Option<State>> {
    let bytes = match read(dir, STATE, 16 * 1024) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let state: State = serde_json::from_slice(&bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "InvalidLifecycleState"))?;
    if !state.valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "InvalidLifecycleState",
        ));
    }
    Ok(Some(state))
}
fn delete_state(dir: &File) -> io::Result<()> {
    match fs::unlinkat(dir, STATE, AtFlags::empty()) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn delete_matching_state(dir: &File, state: &State) -> io::Result<()> {
    if let Some(current) = load_state(dir)?
        && current.pid == state.pid
        && current.identity == state.identity
    {
        delete_state(dir)?;
    }
    Ok(())
}
pub fn process_matches(state: &State) -> io::Result<bool> {
    let f = match File::open(format!("/proc/{}/cmdline", state.pid)) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    read_process_identity(f, state)
}
fn read_process_identity(reader: impl Read, state: &State) -> io::Result<bool> {
    let mut bytes = Vec::new();
    if let Err(error) = reader.take(64 * 1024).read_to_end(&mut bytes) {
        // Linux can retain an opened /proc/<pid>/cmdline descriptor after the
        // process exits, then return ESRCH from read. Disappearance is never
        // proof of identity: report no match and do not authorize a signal.
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(false);
        }
        return Err(error);
    }
    if bytes.is_empty() || bytes.len() == 64 * 1024 {
        return Ok(false);
    }
    let arg = format!("-Djmcl.server.identity={}", state.identity);
    Ok(bytes.split(|b| *b == 0).any(|a| a == arg.as_bytes()))
}
fn status_unlocked(dir: &File) -> io::Result<Status> {
    let state = match load_state(dir) {
        Ok(Some(s)) => s,
        Ok(None) => return Ok(Status::stopped(false)),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            delete_state(dir)?;
            return Ok(Status::stopped(true));
        }
        Err(e) => return Err(e),
    };
    if !process_matches(&state)? {
        delete_state(dir)?;
        return Ok(Status::stopped(true));
    }
    Ok(Status {
        running: true,
        stale_state: false,
        pid: Some(state.pid),
        supervisor_pid: Some(state.supervisor_pid),
        java_path: Some(state.java_path),
        started_at_ms: Some(state.started_at_ms),
        uptime_ms: Some(now_ms().saturating_sub(state.started_at_ms).max(0)),
    })
}
pub fn status(dir: &File) -> Result<Status> {
    let _guard = lock(dir).map_err(|_| "ServerStatusFailed")?;
    status_unlocked(dir).map_err(|_| "ServerStatusFailed")
}
fn validate_command(command: &str) -> Result<()> {
    if command.is_empty()
        || command.len() > 4095
        || command.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r'))
    {
        Err("InvalidServerCommand")
    } else {
        Ok(())
    }
}
fn write_fifo(dir: &File, command: &str) -> Result<()> {
    let mut f = open(
        dir,
        FIFO,
        OFlags::WRONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW,
        0,
    )
    .map_err(|e| match e.raw_os_error() {
        Some(libc::ENOENT | libc::ENXIO | libc::EAGAIN) => "ServerNotRunning",
        _ => "ServerCommandFailed",
    })?;
    if f.metadata().map_err(|_| "ServerCommandFailed")?.mode() & libc::S_IFMT != libc::S_IFIFO {
        return Err("ServerCommandFailed");
    }
    let mut bytes = command.as_bytes().to_vec();
    bytes.push(b'\n');
    f.write_all(&bytes).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            "ServerCommandUnavailable"
        } else {
            "ServerCommandFailed"
        }
    })
}
pub fn command(dir: &File, command: &str) -> Result<u32> {
    validate_command(command)?;
    let _lock = lock(dir).map_err(|_| "ServerCommandFailed")?;
    let status = status_unlocked(dir).map_err(|_| "ServerCommandFailed")?;
    let pid = status.pid.ok_or("ServerNotRunning")?;
    write_fifo(dir, command)?;
    Ok(pid)
}
pub fn read_logs(
    dir: &File,
    cursor: Option<u64>,
    file_id: Option<u64>,
    max_bytes: usize,
) -> Result<Logs> {
    if max_bytes == 0 || max_bytes > MAX_LOG_READ_BYTES {
        return Err("InvalidLogReadSize");
    }
    let mut file = open(
        dir,
        LOG,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW,
        0,
    )
    .map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            "ServerLogsNotFound"
        } else {
            "ServerLogsFailed"
        }
    })?;
    let meta = file.metadata().map_err(|_| "ServerLogsFailed")?;
    if !meta.is_file() {
        return Err("ServerLogsFailed");
    }
    let id = meta.ino();
    let size = meta.len();
    let reset = cursor.is_some_and(|c| c > size || file_id != Some(id));
    let tail = cursor.is_none() || reset;
    let raw_start = if tail {
        size.saturating_sub(max_bytes as u64)
    } else {
        cursor.unwrap()
    };
    let mut bytes = vec![0; (size - raw_start).min(max_bytes as u64) as usize];
    file.seek(SeekFrom::Start(raw_start))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|_| "ServerLogsFailed")?;
    let leading = if raw_start > 0 {
        bytes.iter().take_while(|b| **b & 0xc0 == 0x80).count()
    } else {
        0
    };
    let end = match std::str::from_utf8(&bytes[leading..]) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() => leading + e.valid_up_to(),
        Err(_) => return Err("ServerLogsFailed"),
    };
    let text = String::from_utf8(bytes[leading..end].to_vec()).map_err(|_| "ServerLogsFailed")?;
    let next = raw_start + end as u64;
    Ok(Logs {
        text,
        file_id: id,
        start_cursor: raw_start + leading as u64,
        next_cursor: next,
        eof: next >= size,
        reset,
        truncated: tail && raw_start != 0,
    })
}
fn wait_exit(state: &State, millis: u64) -> io::Result<bool> {
    let start = Instant::now();
    loop {
        if !process_matches(state)? {
            return Ok(true);
        }
        if start.elapsed() >= Duration::from_millis(millis) {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn signal(state: &State, signal: i32) -> io::Result<()> {
    // Revalidate immediately before signalling; never act on an unverified PID.
    if !process_matches(state)? {
        return Ok(());
    }
    let pid = i32::try_from(state.pid).map_err(io::Error::other)?;
    // SAFETY: kill has no pointer parameters; PID was validated against /proc.
    if unsafe { libc::kill(pid, signal) } != 0 {
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::ESRCH) {
            return Err(e);
        }
    }
    Ok(())
}
fn stop_unlocked(dir: &File, grace_ms: u64) -> Result<StopResult> {
    let state = load_state(dir).map_err(|_| "ServerStopFailed")?;
    let Some(state) = state else {
        return Ok(StopResult {
            termination: Termination::NotRunning,
            pid: None,
        });
    };
    if !process_matches(&state).map_err(|_| "ServerStopFailed")? {
        delete_state(dir).map_err(|_| "ServerStopFailed")?;
        return Ok(StopResult {
            termination: Termination::NotRunning,
            pid: None,
        });
    }
    let _ = write_fifo(dir, "stop");
    for (sig, millis, termination) in [
        (None, grace_ms, Termination::Graceful),
        (Some(libc::SIGTERM), 5000, Termination::Terminated),
        (Some(libc::SIGKILL), 2000, Termination::Killed),
    ] {
        if let Some(sig) = sig {
            signal(&state, sig).map_err(|_| "ServerStopFailed")?;
        }
        if wait_exit(&state, millis).map_err(|_| "ServerStopFailed")? {
            delete_matching_state(dir, &state).map_err(|_| "ServerStopFailed")?;
            return Ok(StopResult {
                termination,
                pid: Some(state.pid),
            });
        }
    }
    Err("ServerDidNotExit")
}
pub fn stop(dir: &File, grace_ms: u64) -> Result<StopResult> {
    if grace_ms > MAX_GRACE_MS {
        return Err("InvalidGracePeriod");
    }
    let _lock = lock(dir).map_err(|_| "ServerStopFailed")?;
    stop_unlocked(dir, grace_ms)
}

fn ensure_fifo(dir: &File) -> io::Result<()> {
    match fs::mknodat(dir, FIFO, fs::FileType::Fifo, Mode::from_raw_mode(0o600), 0) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
        Err(e) => return Err(e.into()),
    }
    let stat = fs::statat(dir, FIFO, AtFlags::SYMLINK_NOFOLLOW)?;
    if fs::FileType::from_raw_mode(stat.st_mode) != fs::FileType::Fifo {
        return Err(io::Error::other("NotFifo"));
    }
    Ok(())
}
fn validate_java(java: &str) -> Result<()> {
    if java.is_empty() || java.len() > 4096 || java.contains('\0') {
        Err("InvalidJavaPath")
    } else {
        Ok(())
    }
}
pub fn start(dir: &File, java: &str) -> Result<StartResult> {
    validate_java(java)?;
    let _lock = lock(dir).map_err(|_| "ServerStartFailed")?;
    start_unlocked(dir, java)
}
pub fn restart(dir: &File, java: &str, grace_ms: u64) -> Result<StartResult> {
    if grace_ms > MAX_GRACE_MS {
        return Err("InvalidGracePeriod");
    }
    validate_java(java)?;
    let _lock = lock(dir).map_err(|_| "ServerRestartFailed")?;
    stop_unlocked(dir, grace_ms).map_err(|e| {
        if e == "ServerDidNotExit" {
            e
        } else {
            "ServerRestartFailed"
        }
    })?;
    start_unlocked(dir, java).map_err(|e| {
        if e == "ServerAlreadyRunning" {
            e
        } else {
            "ServerRestartFailed"
        }
    })
}
fn start_unlocked(dir: &File, java: &str) -> Result<StartResult> {
    if status_unlocked(dir)
        .map_err(|_| "ServerStartFailed")?
        .running
    {
        return Err("ServerAlreadyRunning");
    }
    let run = || -> io::Result<StartResult> {
        ensure_fifo(dir)?;
        mkdir(dir, "logs")?;
        let mut random = [0; 16];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let identity = format!("{:032x}", u128::from_be_bytes(random));
        let path = std::fs::read_link(format!("/proc/self/fd/{}", dir.as_raw_fd()))?;
        let exe = std::env::current_exe()?;
        let output = std::process::Command::new(exe)
            .arg("__server_bootstrap")
            .arg(path)
            .arg(java)
            .arg(&identity)
            .output()?;
        if !output.status.success() || output.stdout.len() > 512 {
            return Err(io::Error::other("BootstrapFailed"));
        }
        let line = std::str::from_utf8(&output.stdout)
            .map_err(io::Error::other)?
            .trim();
        let pid: u32 = line
            .strip_prefix("OK ")
            .ok_or_else(|| io::Error::other("BootstrapFailed"))?
            .parse()
            .map_err(io::Error::other)?;
        let state = load_state(dir)?.ok_or_else(|| io::Error::other("MissingState"))?;
        if state.pid != pid || state.identity != identity || !process_matches(&state)? {
            return Err(io::Error::other("InvalidState"));
        }
        Ok(StartResult {
            pid,
            supervisor_pid: state.supervisor_pid,
            started_at_ms: state.started_at_ms,
        })
    };
    run().map_err(|_| "ServerStartFailed")
}
/// Dispatch before public CLI/JSONL. Returns None for ordinary commands.
/// Bootstrap detaches its session; the supervisor never inherits caller streams.
pub fn dispatch_hidden(args: &[String]) -> Option<i32> {
    let command = args.first()?.as_str();
    if !matches!(command, "__server_bootstrap" | "__server_supervisor") {
        return None;
    }
    let result = if args.len() != 4 {
        Err(io::Error::other("InvalidArguments"))
    } else if command == "__server_bootstrap" {
        bootstrap(&args[1..])
    } else {
        supervisor(&args[1..])
    };
    match result {
        Ok(()) => Some(0),
        Err(_) => {
            let _ = writeln!(io::stdout(), "ERR ServerStartFailed");
            Some(1)
        }
    }
}
fn handshake(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    for byte in io::BufReader::new(reader.take(513)).bytes() {
        let byte = byte?;
        if byte == b'\n' {
            return Ok(line);
        }
        line.push(byte);
    }
    Err(io::Error::other("InvalidHandshake"))
}
fn bootstrap(args: &[String]) -> io::Result<()> {
    // SAFETY: setsid has no pointer parameters, runs in dedicated bootstrap process.
    if unsafe { libc::setsid() } < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("__server_supervisor")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let line = handshake(
        child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("NoPipe"))?,
    )?;
    io::stdout().write_all(&line)?;
    io::stdout().write_all(b"\n")?;
    io::stdout().flush()?;
    if !line.starts_with(b"OK ") {
        let _ = child.wait();
        return Err(io::Error::other("SupervisorFailed"));
    }
    Ok(()) // deliberately do not wait: supervisor is independent of request lifetime
}
struct KillOnDrop(Option<std::process::Child>);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn open_log(dir: &File) -> io::Result<File> {
    let f = open(
        dir,
        LOG,
        OFlags::WRONLY | OFlags::APPEND | OFlags::CREATE | OFlags::NONBLOCK | OFlags::NOFOLLOW,
        0o600,
    )?;
    if !f.metadata()?.is_file() {
        return Err(io::Error::other("NotFile"));
    }
    Ok(f)
}
fn rotate_log(dir: &File) -> io::Result<File> {
    match fs::unlinkat(dir, "logs/console.log.2", AtFlags::empty()) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => {}
        Err(e) => return Err(e.into()),
    }
    for (a, b) in [
        ("logs/console.log.1", "logs/console.log.2"),
        (LOG, "logs/console.log.1"),
    ] {
        match fs::renameat(dir, a, dir, b) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(e) => return Err(e.into()),
        }
    }
    open_log(dir)
}
fn supervisor(args: &[String]) -> io::Result<()> {
    let dir = File::from(fs::open(
        args[0].as_str(),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let fifo = open(&dir, FIFO, OFlags::RDWR | OFlags::NOFOLLOW, 0)?;
    if fifo.metadata()?.mode() & libc::S_IFMT != libc::S_IFIFO {
        return Err(io::Error::other("NotFifo"));
    }
    let installation = super::store::load_installation(&dir)?
        .ok_or_else(|| io::Error::other("ServerNotInstalled"))?;
    let mut command = std::process::Command::new(&args[1]);
    command.arg(format!("-Djmcl.server.identity={}", args[2]));
    match installation.launch_mode {
        super::store::LaunchMode::Jar => {
            command.arg("-jar").arg(&installation.launch_file);
        }
        super::store::LaunchMode::ArgsFile => {
            command.arg(format!("@{}", installation.launch_file));
        }
    }
    // fchdir binds spawn to the checked directory even if its name is swapped.
    use std::os::unix::process::CommandExt;
    let cwd = dir.as_raw_fd();
    // SAFETY: the closure uses only async-signal-safe fchdir and errno after fork.
    unsafe {
        command.pre_exec(move || {
            if libc::fchdir(cwd) != 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let child = command
        .arg("nogui")
        .stdin(fifo)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let state = State {
        schema_version: 1,
        pid: child.id(),
        supervisor_pid: std::process::id(),
        identity: args[2].clone(),
        java_path: args[1].clone(),
        started_at_ms: now_ms(),
    };
    let mut child = KillOnDrop(Some(child));
    let child_ref = child.0.as_mut().unwrap();
    let out = child_ref.stdout.take().unwrap();
    let err = child_ref.stderr.take().unwrap();
    let mut log = open_log(&dir)?;
    let mut bytes = serde_json::to_vec(&state)?;
    bytes.push(b'\n');
    atomic_write(&dir, STATE, &bytes)?;
    let outcome = (|| -> io::Result<()> {
        writeln!(io::stdout(), "OK {}", state.pid)?;
        io::stdout().flush()?;
        // SAFETY: after handshake stdout must not keep bootstrap's pipe alive.
        unsafe {
            libc::close(libc::STDOUT_FILENO);
        }
        let streams = [out.as_raw_fd(), err.as_raw_fd()];
        let mut polls = streams.map(|fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
        let pump = (|| -> io::Result<()> {
            let mut live = 2;
            let mut buffer = [0; 4096];
            while live > 0 {
                // SAFETY: pollfds point to initialized valid array; owned pipes stay alive.
                if unsafe { libc::poll(polls.as_mut_ptr(), 2, -1) } < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                }
                for poll in &mut polls {
                    if poll.fd < 0 || poll.revents == 0 {
                        continue;
                    }
                    // SAFETY: buffer is writable for its length and fd is an owned pipe.
                    let n =
                        unsafe { libc::read(poll.fd, buffer.as_mut_ptr().cast(), buffer.len()) };
                    if n < 0 {
                        let e = io::Error::last_os_error();
                        if e.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(e);
                    }
                    if n == 0 {
                        poll.fd = -1;
                        live -= 1;
                        continue;
                    }
                    let n = n as usize;
                    let size = log.metadata()?.len();
                    if size >= MAX_LOG_FILE_BYTES || n as u64 > MAX_LOG_FILE_BYTES - size {
                        log = rotate_log(&dir)?;
                    }
                    log.write_all(&buffer[..n])?;
                }
            }
            Ok(())
        })();
        if pump.is_ok() {
            child.0.as_mut().unwrap().wait()?;
            child.0.take();
        }
        pump
    })();
    // Drop kills on error before cleaning the PID record.
    drop(child);
    let _lock = lock(&dir)?;
    let cleanup = delete_matching_state(&dir, &state);
    outcome.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> (tempfile::TempDir, File) {
        let tmp = tempfile::tempdir().unwrap();
        let f = File::open(tmp.path()).unwrap();
        (tmp, f)
    }
    fn state() -> State {
        State {
            schema_version: 1,
            pid: std::process::id(),
            supervisor_pid: std::process::id(),
            identity: "0123456789abcdef0123456789abcdef".into(),
            java_path: "/synthetic/java".into(),
            started_at_ms: now_ms(),
        }
    }
    #[test]
    fn identity_read_after_owned_child_exit_is_not_a_stop_failure() {
        let mut state = state();
        let identity = format!("-Djmcl.server.identity={}", state.identity);
        let child = std::process::Command::new("/bin/sh")
            .args(["-c", "read line", identity.as_str()])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut child = KillOnDrop(Some(child));
        let process = child.0.as_mut().unwrap();
        state.pid = process.id();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !process_matches(&state).unwrap() {
            assert!(
                Instant::now() < deadline,
                "synthetic child did not exec with identity"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let identity_file = File::open(format!("/proc/{}/cmdline", state.pid)).unwrap();
        process.kill().unwrap();
        process.wait().unwrap();
        child.0.take();
        // Keep the opened proc descriptor across exit: the kernel now returns
        // ESRCH from read, rather than ENOENT from a new pathname lookup.
        assert!(!read_process_identity(identity_file, &state).unwrap());
    }
    #[test]
    fn identity_read_only_treats_process_disappearance_as_no_match() {
        struct Failed(i32);
        impl Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from_raw_os_error(self.0))
            }
        }
        let state = state();
        for errno in [libc::EACCES, libc::EIO] {
            assert_eq!(
                read_process_identity(Failed(errno), &state)
                    .unwrap_err()
                    .raw_os_error(),
                Some(errno)
            );
        }
        assert!(!read_process_identity(Failed(libc::ESRCH), &state).unwrap());
        let exact = format!("java\0-Djmcl.server.identity={}\0nogui\0", state.identity);
        assert!(read_process_identity(exact.as_bytes(), &state).unwrap());
        // Even partially read matching bytes followed by disappearance cannot
        // become identity evidence and authorize a later signal.
        assert!(
            !read_process_identity(exact.as_bytes().chain(Failed(libc::ESRCH)), &state).unwrap()
        );
        assert_eq!(
            read_process_identity(exact.as_bytes().chain(Failed(libc::EIO)), &state)
                .unwrap_err()
                .raw_os_error(),
            Some(libc::EIO)
        );
        assert!(
            !read_process_identity(b"java\0-Djmcl.server.identity=other\0".as_slice(), &state)
                .unwrap()
        );
        assert!(!read_process_identity(&[][..], &state).unwrap());
        assert!(!read_process_identity(vec![b'x'; 64 * 1024].as_slice(), &state).unwrap());
    }
    #[test]
    fn stale_identity_never_signals_real_pid() {
        let (tmp, dir) = root();
        mkdir(&dir, ".jmcl").unwrap();
        let s = state();
        atomic_write(&dir, STATE, &serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(!process_matches(&s).unwrap());
        let result = stop(&dir, 0).unwrap();
        assert_eq!(result.termination, Termination::NotRunning);
        assert!(!tmp.path().join(STATE).exists());
    }
    #[test]
    fn malformed_state_cleans_once() {
        let (_tmp, dir) = root();
        mkdir(&dir, ".jmcl").unwrap();
        atomic_write(&dir, STATE, b"bad").unwrap();
        assert!(status(&dir).unwrap().stale_state);
        assert!(!status(&dir).unwrap().stale_state);
    }
    #[test]
    fn state_validation() {
        let mut s = state();
        assert!(s.valid());
        s.pid = 1;
        assert!(!s.valid());
        s.pid = 2;
        s.identity.push('a');
        assert!(!s.valid());
    }
    #[test]
    fn logs_utf8_cursors_rotation_and_limits() {
        let (_tmp, dir) = root();
        mkdir(&dir, "logs").unwrap();
        atomic_write(&dir, LOG, "你好abc".as_bytes()).unwrap();
        let tail = read_logs(&dir, None, None, 5).unwrap();
        assert_eq!(tail.text, "abc");
        assert_eq!(tail.start_cursor, 6);
        assert_eq!(tail.next_cursor, 9);
        assert!(tail.truncated && tail.eof);
        let partial = read_logs(&dir, Some(0), Some(tail.file_id), 4).unwrap();
        assert_eq!(partial.text, "你");
        assert_eq!(partial.next_cursor, 3);
        assert!(!partial.eof);
        atomic_write(&dir, LOG, b"rotated").unwrap();
        let reset = read_logs(&dir, Some(9), Some(tail.file_id), 5).unwrap();
        assert!(reset.reset);
        assert_eq!(reset.text, "tated");
        assert!(read_logs(&dir, None, None, 0).is_err());
        assert!(read_logs(&dir, None, None, MAX_LOG_READ_BYTES + 1).is_err());
        atomic_write(&dir, LOG, b"a\xffb").unwrap();
        assert_eq!(
            read_logs(&dir, None, None, 100).unwrap_err(),
            "ServerLogsFailed"
        );
    }
    #[test]
    fn fifo_and_log_leaf_symlinks_are_rejected() {
        let (tmp, dir) = root();
        mkdir(&dir, ".jmcl").unwrap();
        mkdir(&dir, "logs").unwrap();
        std::os::unix::fs::symlink("/dev/null", tmp.path().join(FIFO)).unwrap();
        assert!(ensure_fifo(&dir).is_err());
        std::os::unix::fs::symlink("/dev/null", tmp.path().join(LOG)).unwrap();
        assert!(read_logs(&dir, None, None, 10).is_err());
        assert!(open_log(&dir).is_err());
    }
    #[test]
    fn log_rotation_keeps_two_generations() {
        let (_tmp, dir) = root();
        mkdir(&dir, "logs").unwrap();
        atomic_write(&dir, LOG, b"one").unwrap();
        let mut f = rotate_log(&dir).unwrap();
        f.write_all(b"two").unwrap();
        drop(f);
        let mut f = rotate_log(&dir).unwrap();
        f.write_all(b"three").unwrap();
        drop(f);
        assert_eq!(read(&dir, "logs/console.log.2", 100).unwrap(), b"one");
        assert_eq!(read(&dir, "logs/console.log.1", 100).unwrap(), b"two");
        assert_eq!(read(&dir, LOG, 100).unwrap(), b"three");
    }
    #[test]
    fn fifo_commands_have_one_newline() {
        let (_tmp, dir) = root();
        mkdir(&dir, ".jmcl").unwrap();
        ensure_fifo(&dir).unwrap();
        let mut reader = open(&dir, FIFO, OFlags::RDWR | OFlags::NONBLOCK, 0).unwrap();
        write_fifo(&dir, "say test").unwrap();
        let mut b = [0; 32];
        let n = reader.read(&mut b).unwrap();
        assert_eq!(&b[..n], b"say test\n");
        for bad in ["", "a\nb", "a\rb", "a\0b"] {
            assert!(validate_command(bad).is_err());
        }
        assert!(validate_command(&"a".repeat(4096)).is_err());
    }
    #[test]
    fn stopped_guard_serializes() {
        let (_tmp, dir) = root();
        let guard = lock_stopped(&dir).unwrap();
        let f = open(&dir, ".jmcl/lifecycle.lock", OFlags::RDWR, 0).unwrap();
        assert!(fs::flock(&f, fs::FlockOperation::NonBlockingLockExclusive).is_err());
        drop(guard);
        assert!(fs::flock(&f, fs::FlockOperation::NonBlockingLockExclusive).is_ok());
    }
    #[test]
    fn numeric_record_compatibility_and_same_parent_atomic_write() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, dir) = root();
        mkdir(&dir, ".jmcl").unwrap();
        let bytes=serde_json::to_vec(&serde_json::json!({"schema_version":"1.0","pid":"12e1","supervisor_pid":123.0,"identity":"0123456789abcdef0123456789abcdef","java_path":"/synthetic/java","started_at_ms":"123456789"})).unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = atomic_write(&dir, STATE, &bytes);
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        result.unwrap();
        let state = load_state(&dir).unwrap().unwrap();
        assert_eq!(state.pid, 120);
        assert_eq!(state.supervisor_pid, 123);
        assert_eq!(state.started_at_ms, 123456789);
    }
}

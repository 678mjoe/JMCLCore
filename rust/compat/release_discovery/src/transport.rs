//! Line-oriented transports for the core's JSON Lines RPC channel.
//!
//! The session layer only knows [`LineTransport`]: write one request line,
//! read inbound items. The local implementation spawns `jmcl-core rpc` as a
//! child process. A future SSH transport (running `jmcl-core rpc` on a remote
//! host over an SSH channel) implements the same trait and plugs into
//! sessions without changes.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

#[cfg(windows)]
pub(crate) fn hide_child_console(command: &mut Command) {
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
}

/// Safe, serializable classification for transport failures. Messages are
/// suitable for the UI and must never contain process output or destinations.
#[derive(Debug, Clone)]
pub struct TransportFailure {
    pub code: String,
    pub message: String,
}

impl TransportFailure {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn local(message: impl Into<String>) -> Self {
        Self::new("TRANSPORT_ERROR", message)
    }
}

/// One inbound item from a transport.
#[derive(Debug)]
pub enum TransportEvent {
    /// One line of the RPC stream (a JSON event).
    Line(String),
    /// One diagnostics line (child stderr).
    Diagnostic(String),
    /// The RPC stream ended; carries a human-readable reason.
    Closed(TransportFailure),
}

/// A bidirectional JSON Lines channel to one core endpoint.
#[async_trait]
pub trait LineTransport: Send {
    /// Write one request line; the transport appends the newline.
    async fn write_line(&mut self, line: &str) -> Result<(), TransportFailure>;
    /// Wait for the next inbound item. Yields [`TransportEvent::Closed`]
    /// once the stream ends.
    async fn next_event(&mut self) -> TransportEvent;
    /// Half-close the request stream; the core exits after draining.
    async fn close(&mut self) -> Result<(), TransportFailure>;
    /// Immediately terminate this transport. Must be idempotent.
    async fn abort(&mut self) -> Result<(), TransportFailure>;
    /// Error code used when a complete stdout line is not valid protocol JSON.
    fn protocol_error_code(&self) -> &'static str {
        "TRANSPORT_ERROR"
    }
}

#[cfg(windows)]
const CORE_EXE: &str = "jmcl-core.exe";
#[cfg(not(windows))]
const CORE_EXE: &str = "jmcl-core";

fn find_core_near(executable: &Path) -> Option<PathBuf> {
    for dir in executable.ancestors().skip(1).take(8) {
        // The adjacent binary only exists in packaged builds (Tauri externalBin
        // places the sidecar next to the executable). Debug builds skip it so a
        // stray copy in target/ can never shadow the canonical dev binary below.
        if !cfg!(debug_assertions) {
            let adjacent = dir.join(CORE_EXE);
            if adjacent.is_file() {
                return Some(adjacent);
            }
        }
        let development = dir.join("backend-binaries").join(CORE_EXE);
        if development.is_file() {
            return Some(development);
        }
    }
    None
}

/// Resolve the core binary: explicit path → `JMCL_CORE` env → the app's
/// executable directory and its ancestors. Release builds check each ancestor
/// for a bundled sidecar; every build then falls back to the development
/// `backend-binaries` directory.
pub fn resolve_core_binary(explicit: Option<&str>) -> Result<PathBuf, String> {
    let chosen = explicit
        .map(str::to_owned)
        .or_else(|| std::env::var("JMCL_CORE").ok())
        .filter(|p| !p.is_empty());
    if let Some(path) = chosen {
        let path = PathBuf::from(path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("core binary not found at {}", path.display()))
        };
    }
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot locate current executable: {e}"))?;
    if let Some(path) = find_core_near(&exe) {
        return Ok(path);
    }
    Err(format!(
        "{CORE_EXE} not found near the application or in backend-binaries; set JMCL_CORE or pass binary_path"
    ))
}

/// Transport over a locally spawned `jmcl-core rpc` child process.
pub struct LocalProcessTransport {
    stdin: Option<ChildStdin>,
    rx: mpsc::UnboundedReceiver<TransportEvent>,
    child: Child,
}

impl LocalProcessTransport {
    /// Spawn `program rpc` and start its stdout/stderr reader tasks.
    ///
    /// `working_dir` becomes the child's CWD; the core's CWD-relative
    /// defaults (e.g. the `accounts` registry, contract §6) resolve there.
    pub async fn spawn(
        program: &Path,
        args: &[&str],
        working_dir: Option<&Path>,
    ) -> Result<Self, String> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        hide_child_console(&mut command);
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {e}", program.display()))?;

        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let (tx, rx) = mpsc::unbounded_channel();

        let stdout_tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if stdout_tx.send(TransportEvent::Line(line)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = stdout_tx.send(TransportEvent::Closed(TransportFailure::new(
                            "CORE_RPC_EOF",
                            "core closed its RPC stream",
                        )));
                        break;
                    }
                    Err(e) => {
                        let _ = stdout_tx.send(TransportEvent::Closed(TransportFailure::new(
                            "CORE_RPC_READ_FAILED",
                            format!("RPC stream read error: {e}"),
                        )));
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if tx.send(TransportEvent::Diagnostic(line)).is_err() {
                            break;
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        });

        Ok(Self {
            stdin: Some(stdin),
            rx,
            child,
        })
    }
}

#[async_trait]
impl LineTransport for LocalProcessTransport {
    fn protocol_error_code(&self) -> &'static str {
        "CORE_PROTOCOL_ERROR"
    }
    async fn write_line(&mut self, line: &str) -> Result<(), TransportFailure> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| TransportFailure::new("SESSION_CLOSED", "session is closed"))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| TransportFailure::new("CORE_RPC_WRITE_FAILED", "write to core failed"))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|_| TransportFailure::new("CORE_RPC_WRITE_FAILED", "write to core failed"))?;
        stdin
            .flush()
            .await
            .map_err(|_| TransportFailure::new("CORE_RPC_WRITE_FAILED", "flush to core failed"))
    }

    async fn next_event(&mut self) -> TransportEvent {
        self.rx.recv().await.unwrap_or_else(|| {
            TransportEvent::Closed(TransportFailure::new(
                "CORE_RPC_READERS_STOPPED",
                "reader tasks stopped",
            ))
        })
    }

    async fn close(&mut self) -> Result<(), TransportFailure> {
        // Dropping stdin sends EOF; the core drains and exits.
        self.stdin.take();
        if timeout(Duration::from_secs(1), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
        }
        Ok(())
    }

    async fn abort(&mut self) -> Result<(), TransportFailure> {
        self.stdin.take();
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        Ok(())
    }
}

impl Drop for LocalProcessTransport {
    fn drop(&mut self) {
        // Best-effort: if the core is still alive (aborted op, hang), kill it.
        let _ = self.child.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn local_eof_and_protocol_classification_are_static() {
        let mut transport =
            LocalProcessTransport::spawn(Path::new("/bin/sh"), &["-c", "exit 0"], None)
                .await
                .unwrap();
        assert_eq!(transport.protocol_error_code(), "CORE_PROTOCOL_ERROR");
        let event = timeout(Duration::from_secs(3), transport.next_event())
            .await
            .unwrap();
        assert!(
            matches!(event, TransportEvent::Closed(TransportFailure {code, ..}) if code == "CORE_RPC_EOF")
        );
        transport.close().await.unwrap();
        assert_eq!(
            transport
                .write_line("sentinel-secret")
                .await
                .unwrap_err()
                .code,
            "SESSION_CLOSED"
        );
    }
    #[test]
    fn finds_development_binary_directory() {
        let root = std::env::temp_dir().join(format!("jmcl-core-resolver-{}", std::process::id()));
        let executable = root.join("src-tauri/target/debug/jmcl");
        let core = root.join("backend-binaries").join(CORE_EXE);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(core.parent().unwrap()).unwrap();
        std::fs::write(&core, []).unwrap();

        assert_eq!(find_core_near(&executable), Some(core));

        std::fs::remove_dir_all(root).unwrap();
    }

    /// Debug builds never pick up an adjacent `jmcl-core` — only packaged
    /// (release) sidecars may. A stale copy next to a dev executable must not
    /// shadow the development `backend-binaries` binary.
    #[cfg(debug_assertions)]
    #[test]
    fn debug_builds_ignore_adjacent_binary() {
        let root = std::env::temp_dir().join(format!("jmcl-core-adjacent-{}", std::process::id()));
        let executable = root.join("src-tauri/target/debug/jmcl");
        let adjacent = executable.parent().unwrap().join(CORE_EXE);
        let development = root.join("backend-binaries").join(CORE_EXE);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(development.parent().unwrap()).unwrap();
        std::fs::write(&adjacent, []).unwrap();
        std::fs::write(&development, []).unwrap();

        assert_eq!(find_core_near(&executable), Some(development));

        // With no backend-binaries anywhere, the adjacent copy alone is not enough.
        std::fs::remove_dir_all(root.join("backend-binaries")).unwrap();
        assert_eq!(find_core_near(&executable), None);

        std::fs::remove_dir_all(root).unwrap();
    }
}

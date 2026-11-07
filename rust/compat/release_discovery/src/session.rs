//! Protocol v1 session state machine on top of a [`LineTransport`].
//!
//! Protocol v1 is synchronous: one request in flight per session. A request
//! holds the transport lock for its whole lifetime, so concurrent callers
//! queue naturally. Use multiple sessions (see `pool`) for real concurrency.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{watch, Mutex};

use crate::transport::{LineTransport, TransportEvent, TransportFailure};

/// Wire protocol version this GUI speaks (contract §2).
pub const PROTOCOL_VERSION: u32 = 1;
/// Maximum request line size accepted by the core.
pub const MAX_REQUEST_LINE: usize = 64 * 1024;
/// Bound best-effort transport cleanup so semantic errors are returned promptly.
const ABORT_CLEANUP_DEADLINE: Duration = Duration::from_secs(1);

/// Non-terminal items surfaced to the GUI while a request runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SessionEvent {
    /// A core progress/lifecycle event (`started`, `progress`, `file`,
    /// `retry`, `stdout`, `stderr`, ...), forwarded verbatim.
    Event(Value),
    /// A core stderr diagnostics line.
    Diagnostic(String),
}

/// Why a request failed. Serializes to the GUI as `{kind, ...}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionError {
    /// The core rejected the request; `code` is a stable contract code.
    Rpc { code: String, message: String },
    /// Session/transport failure: spawn, write, EOF, protocol violation.
    Transport { code: String, message: String },
}

impl SessionError {
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport {
            code: "TRANSPORT_ERROR".into(),
            message: message.into(),
        }
    }

    pub fn coded(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Transport {
            code: code.into(),
            message: message.into(),
        }
    }

    pub(crate) fn from_transport(error: TransportFailure) -> Self {
        Self::coded(error.code, error.message)
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc { code, message } => write!(f, "{code}: {message}"),
            Self::Transport { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// `core.version` handshake result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreIdentity {
    pub name: String,
    pub version: String,
    pub protocol: u64,
}

/// One synchronous RPC session with a core endpoint.
pub struct Session {
    transport: Mutex<Box<dyn LineTransport>>,
    protocol_error_code: &'static str,
    sequence: AtomicU64,
    closed: AtomicBool,
    credentialed: AtomicBool,
    cancel: watch::Sender<bool>,
    identity: CoreIdentity,
}

impl Session {
    async fn best_effort_abort(transport: &mut Box<dyn LineTransport>) {
        let _ = tokio::time::timeout(ABORT_CLEANUP_DEADLINE, transport.abort()).await;
    }

    /// Establish a session: wrap the transport and run the `core.version`
    /// handshake, rejecting incompatible protocol versions (contract §3).
    pub async fn establish(transport: Box<dyn LineTransport>) -> Result<Self, SessionError> {
        Self::establish_inner(transport, None).await
    }

    /// Establish a protocol session within a fixed deadline. On timeout the
    /// owned transport is immediately aborted and reaped where supported.
    pub async fn establish_with_timeout(
        transport: Box<dyn LineTransport>,
        duration: Duration,
    ) -> Result<Self, SessionError> {
        Self::establish_inner(transport, Some(duration)).await
    }

    async fn establish_inner(
        transport: Box<dyn LineTransport>,
        deadline: Option<Duration>,
    ) -> Result<Self, SessionError> {
        let protocol_error_code = transport.protocol_error_code();
        let mut session = Self {
            transport: Mutex::new(transport),
            protocol_error_code,
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 0,
            },
        };
        let request = session.request("core.version", json!({}), |_| {});
        let result = if let Some(deadline) = deadline {
            match tokio::time::timeout(deadline, request).await {
                Ok(result) => result?,
                Err(_) => {
                    session.closed.store(true, Ordering::SeqCst);
                    session.cancel.send_replace(true);
                    let mut transport = session.transport.lock().await;
                    Self::best_effort_abort(&mut transport).await;
                    return Err(SessionError::coded("SSH_TIMEOUT", "The SSH connection did not complete its JMCLCore handshake within 15 seconds."));
                }
            }
        } else {
            request.await?
        };
        let identity: CoreIdentity = match serde_json::from_value(result) {
            Ok(identity) => identity,
            Err(_) => {
                session.closed.store(true, Ordering::SeqCst);
                session.cancel.send_replace(true);
                let mut transport = session.transport.lock().await;
                Self::best_effort_abort(&mut transport).await;
                return Err(SessionError::coded(
                    session.protocol_error_code,
                    "The remote core sent an invalid identity response.",
                ));
            }
        };
        if identity.protocol != u64::from(PROTOCOL_VERSION) {
            session.closed.store(true, Ordering::SeqCst);
            session.cancel.send_replace(true);
            let mut transport = session.transport.lock().await;
            Self::best_effort_abort(&mut transport).await;
            return Err(SessionError::coded(
                "UNSUPPORTED_PROTOCOL",
                "This endpoint uses an unsupported JMCLCore protocol version.",
            ));
        }
        session.identity = identity;
        Ok(session)
    }

    pub fn identity(&self) -> &CoreIdentity {
        &self.identity
    }

    /// Send one request and wait for its terminal event. Progress events and
    /// diagnostics are passed to `on_event` as they arrive.
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        on_event: impl FnMut(SessionEvent),
    ) -> Result<Value, SessionError> {
        self.request_inner(method, params, on_event, false).await
    }

    /// A credential-bearing session is single-use, including callers already
    /// queued on its transport. Never let a later request read a secret tail.
    pub async fn request_credentialed(
        &self,
        method: &str,
        params: Value,
        on_event: impl FnMut(SessionEvent),
    ) -> Result<Value, SessionError> {
        if self.credentialed.swap(true, Ordering::SeqCst) {
            return Err(SessionError::coded(
                "SESSION_CLOSED",
                "This session is closed.",
            ));
        }
        let result = self.request_inner(method, params, on_event, true).await;
        self.signal_close();
        result
    }

    async fn request_inner(
        &self,
        method: &str,
        params: Value,
        mut on_event: impl FnMut(SessionEvent),
        credentialed: bool,
    ) -> Result<Value, SessionError> {
        let mut cancel = self.cancel.subscribe();
        if *cancel.borrow()
            || self.closed.load(Ordering::SeqCst)
            || (!credentialed && self.credentialed.load(Ordering::SeqCst))
        {
            return Err(SessionError::coded(
                "SESSION_CLOSED",
                "This session is closed.",
            ));
        }
        // Held for the whole request: protocol v1 allows one request at a
        // time per session, so concurrent callers queue here.
        let mut transport = tokio::select! {
            biased;
            _ = cancel.changed() => return Err(SessionError::coded("SESSION_CANCELLED", "The session request was cancelled.")),
            transport = self.transport.lock() => transport,
        };
        if *cancel.borrow()
            || self.closed.load(Ordering::SeqCst)
            || (!credentialed && self.credentialed.load(Ordering::SeqCst))
        {
            return Err(SessionError::coded(
                "SESSION_CANCELLED",
                "The session request was cancelled.",
            ));
        }

        let id = format!("req-{}", self.sequence.fetch_add(1, Ordering::Relaxed));
        let line = json!({
            "protocol": PROTOCOL_VERSION,
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string();
        if line.len() > MAX_REQUEST_LINE {
            return Err(SessionError::transport(format!(
                "request exceeds {MAX_REQUEST_LINE} bytes"
            )));
        }
        tokio::select! {
            biased;
            _ = cancel.changed() => { Self::best_effort_abort(&mut transport).await; return Err(SessionError::coded("SESSION_CANCELLED", "The session request was cancelled.")); }
            result = transport.write_line(&line) => result.map_err(SessionError::from_transport)?,
        }

        loop {
            let incoming = tokio::select! {
                biased;
                _ = cancel.changed() => { Self::best_effort_abort(&mut transport).await; return Err(SessionError::coded("SESSION_CANCELLED", "The session request was cancelled.")); }
                event = transport.next_event() => event,
            };
            match incoming {
                TransportEvent::Line(line) => {
                    let event: Value = match serde_json::from_str(&line) {
                        Ok(event) => event,
                        Err(_) => {
                            let code = self.protocol_error_code;
                            self.closed.store(true, Ordering::SeqCst);
                            self.cancel.send_replace(true);
                            Self::best_effort_abort(&mut transport).await;
                            return Err(SessionError::coded(
                                code,
                                "The endpoint sent invalid RPC data.",
                            ));
                        }
                    };
                    let event_id = event.get("id").and_then(Value::as_str);
                    let kind = event
                        .get("event")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if event_id != Some(id.as_str()) {
                        // `id: null` errors flag malformed requests; our
                        // serializer never produces them, so seeing one means
                        // a contract drift. Anything else with a foreign id is
                        // a v1 violation — neither belongs to this request.
                        if kind == "error" && event_id.is_none() {
                            let code = self.protocol_error_code;
                            self.closed.store(true, Ordering::SeqCst);
                            self.cancel.send_replace(true);
                            Self::best_effort_abort(&mut transport).await;
                            return Err(SessionError::coded(
                                code,
                                "The endpoint sent an invalid RPC error event.",
                            ));
                        }
                        continue;
                    }
                    match kind {
                        "result" => return Ok(event.get("result").cloned().unwrap_or(Value::Null)),
                        "error" => {
                            let code = event
                                .pointer("/error/code")
                                .and_then(Value::as_str)
                                .unwrap_or("UNKNOWN")
                                .to_string();
                            let message = event
                                .pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("core error")
                                .to_string();
                            return Err(SessionError::Rpc { code, message });
                        }
                        "" => {
                            let code = self.protocol_error_code;
                            self.closed.store(true, Ordering::SeqCst);
                            self.cancel.send_replace(true);
                            Self::best_effort_abort(&mut transport).await;
                            return Err(SessionError::coded(
                                code,
                                "The endpoint sent an incomplete RPC event.",
                            ));
                        }
                        _ => on_event(SessionEvent::Event(event)),
                    }
                }
                TransportEvent::Diagnostic(line) => on_event(SessionEvent::Diagnostic(line)),
                TransportEvent::Closed(reason) => {
                    self.closed.store(true, Ordering::SeqCst);
                    return Err(SessionError::from_transport(reason));
                }
            }
        }
    }

    /// Cancel the in-flight request, then close the transport gracefully when idle.
    pub(crate) fn signal_close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.cancel.send_replace(true);
    }

    pub async fn close(&self) {
        self.signal_close();
        let mut transport = self.transport.lock().await;
        let _ = transport.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportFailure;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Notify;

    struct NeverTransport {
        entered: Arc<Notify>,
        block_write: bool,
    }

    struct IdentityTransport {
        sent: bool,
    }

    struct WrongProtocolTransport {
        sent: bool,
        aborted: Arc<AtomicBool>,
    }

    struct HangingAbortTransport {
        entered: Arc<Notify>,
    }

    #[async_trait]
    impl LineTransport for HangingAbortTransport {
        async fn write_line(&mut self, _: &str) -> Result<(), TransportFailure> {
            self.entered.notify_one();
            Ok(())
        }

        async fn next_event(&mut self) -> TransportEvent {
            std::future::pending().await
        }

        async fn close(&mut self) -> Result<(), TransportFailure> {
            Ok(())
        }

        async fn abort(&mut self) -> Result<(), TransportFailure> {
            std::future::pending().await
        }
    }

    #[async_trait]
    impl LineTransport for WrongProtocolTransport {
        async fn write_line(&mut self, _: &str) -> Result<(), TransportFailure> {
            Ok(())
        }

        async fn next_event(&mut self) -> TransportEvent {
            self.sent = true;
            TransportEvent::Line(
                r#"{"id":"req-0","event":"result","result":{"name":"jmcl-core","version":"fake","protocol":2}}"#.into(),
            )
        }

        async fn close(&mut self) -> Result<(), TransportFailure> {
            Ok(())
        }

        async fn abort(&mut self) -> Result<(), TransportFailure> {
            self.aborted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait]
    impl LineTransport for IdentityTransport {
        async fn write_line(&mut self, _: &str) -> Result<(), TransportFailure> {
            Ok(())
        }

        async fn next_event(&mut self) -> TransportEvent {
            if !self.sent {
                self.sent = true;
                TransportEvent::Line(
                    r#"{"id":"req-0","event":"result","result":{"version":"fake"}}"#.into(),
                )
            } else {
                TransportEvent::Closed(TransportFailure::local("closed"))
            }
        }

        async fn close(&mut self) -> Result<(), TransportFailure> {
            Ok(())
        }

        async fn abort(&mut self) -> Result<(), TransportFailure> {
            Ok(())
        }
    }

    #[async_trait]
    impl LineTransport for NeverTransport {
        async fn write_line(&mut self, _: &str) -> Result<(), TransportFailure> {
            self.entered.notify_one();
            if self.block_write {
                std::future::pending::<()>().await;
            }
            Ok(())
        }

        async fn next_event(&mut self) -> TransportEvent {
            std::future::pending().await
        }

        async fn close(&mut self) -> Result<(), TransportFailure> {
            Ok(())
        }

        async fn abort(&mut self) -> Result<(), TransportFailure> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn malformed_local_identity_uses_transport_error_code() {
        let error = Session::establish(Box::new(IdentityTransport { sent: false }))
            .await
            .err()
            .unwrap();
        assert!(matches!(error, SessionError::Transport { code, .. } if code == "TRANSPORT_ERROR"));
    }

    #[tokio::test]
    async fn unsupported_protocol_aborts_transport_before_returning() {
        let aborted = Arc::new(AtomicBool::new(false));
        let error = Session::establish(Box::new(WrongProtocolTransport {
            sent: false,
            aborted: aborted.clone(),
        }))
        .await
        .err()
        .unwrap();
        assert!(
            matches!(error, SessionError::Transport { code, .. } if code == "UNSUPPORTED_PROTOCOL")
        );
        assert!(aborted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn handshake_timeout_returns_even_if_abort_never_finishes() {
        let result = tokio::time::timeout(
            Duration::from_millis(1_300),
            Session::establish_with_timeout(
                Box::new(HangingAbortTransport {
                    entered: Arc::new(Notify::new()),
                }),
                Duration::from_millis(5),
            ),
        )
        .await
        .expect("session establishment must not hang while aborting");

        assert!(
            matches!(result, Err(SessionError::Transport { code, .. }) if code == "SSH_TIMEOUT")
        );
    }

    #[tokio::test]
    async fn close_finishes_after_cancelled_request_abort_deadline() {
        let entered = Arc::new(Notify::new());
        let session = Arc::new(Session {
            transport: Mutex::new(Box::new(HangingAbortTransport {
                entered: entered.clone(),
            })),
            protocol_error_code: "TRANSPORT_ERROR",
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 0,
            },
        });
        let request_session = session.clone();
        let request =
            tokio::spawn(async move { request_session.request("ping", json!({}), |_| {}).await });
        entered.notified().await;

        tokio::time::timeout(Duration::from_millis(1_300), session.close())
            .await
            .expect("close must finish after bounded request abort cleanup");
        assert!(matches!(
            request.await.unwrap(),
            Err(SessionError::Transport { code, .. }) if code == "SESSION_CANCELLED"
        ));
    }

    #[tokio::test]
    async fn close_cancels_a_request_that_never_emits() {
        let entered = Arc::new(Notify::new());
        let session = Arc::new(Session {
            transport: Mutex::new(Box::new(NeverTransport {
                entered: entered.clone(),
                block_write: false,
            })),
            protocol_error_code: "TRANSPORT_ERROR",
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 0,
            },
        });
        let request_session = session.clone();
        let request =
            tokio::spawn(async move { request_session.request("ping", json!({}), |_| {}).await });
        entered.notified().await;

        tokio::time::timeout(std::time::Duration::from_millis(100), session.close())
            .await
            .expect("close must not wait for a response that will never arrive");
        assert!(matches!(
            request.await.unwrap(),
            Err(SessionError::Transport { .. })
        ));
        tokio::time::timeout(std::time::Duration::from_millis(100), session.close())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn queued_request_is_cancelled_when_close_starts() {
        let entered = Arc::new(Notify::new());
        let session = Arc::new(Session {
            transport: Mutex::new(Box::new(NeverTransport {
                entered: entered.clone(),
                block_write: false,
            })),
            protocol_error_code: "TRANSPORT_ERROR",
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 0,
            },
        });
        let first_session = session.clone();
        let first =
            tokio::spawn(async move { first_session.request("first", json!({}), |_| {}).await });
        entered.notified().await;
        let queued_session = session.clone();
        let queued =
            tokio::spawn(async move { queued_session.request("queued", json!({}), |_| {}).await });
        tokio::task::yield_now().await;
        tokio::time::timeout(std::time::Duration::from_millis(100), session.close())
            .await
            .unwrap();
        assert!(first.await.unwrap().is_err());
        assert!(queued.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn close_cancels_a_request_blocked_while_writing() {
        let entered = Arc::new(Notify::new());
        let session = Arc::new(Session {
            transport: Mutex::new(Box::new(NeverTransport {
                entered: entered.clone(),
                block_write: true,
            })),
            protocol_error_code: "TRANSPORT_ERROR",
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 0,
            },
        });
        let request_session = session.clone();
        let request =
            tokio::spawn(async move { request_session.request("ping", json!({}), |_| {}).await });
        entered.notified().await;
        tokio::time::timeout(std::time::Duration::from_millis(100), session.close())
            .await
            .expect("close cancels a write that never completes");
        assert!(request.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn credentialed_session_is_single_use_and_rejects_later_reads() {
        let session = Session {
            transport: Mutex::new(Box::new(IdentityTransport { sent: false })),
            protocol_error_code: "TRANSPORT_ERROR",
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 1,
            },
        };
        assert!(session
            .request_credentialed("mods.install", json!({"api_key":"synthetic"}), |_| {})
            .await
            .is_ok());
        assert!(session.closed.load(Ordering::SeqCst));
        assert!(session
            .request("ping", json!({}), |_| panic!(
                "must not read a diagnostic tail"
            ))
            .await
            .is_err());
        assert!(session
            .request_credentialed("mods.install", json!({}), |_| {})
            .await
            .is_err());
    }

    #[tokio::test]
    async fn ordinary_request_cannot_queue_behind_credentialed_request() {
        let entered = Arc::new(Notify::new());
        let session = Arc::new(Session {
            transport: Mutex::new(Box::new(NeverTransport {
                entered: entered.clone(),
                block_write: false,
            })),
            protocol_error_code: "TRANSPORT_ERROR",
            sequence: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            credentialed: AtomicBool::new(false),
            cancel: watch::channel(false).0,
            identity: CoreIdentity {
                name: String::new(),
                version: String::new(),
                protocol: 1,
            },
        });
        let running = session.clone();
        let request = tokio::spawn(async move {
            running
                .request_credentialed("mods.install", json!({}), |_| {})
                .await
        });
        entered.notified().await;
        let other = tokio::time::timeout(
            Duration::from_millis(100),
            session.request("ping", json!({}), |_| {}),
        )
        .await
        .unwrap();
        assert!(other.is_err());
        session.close().await;
        assert!(request.await.unwrap().is_err());
    }

    #[test]
    fn session_events_use_lowercase_wire_discriminators() {
        let event = serde_json::to_value(SessionEvent::Event(json!({
            "event": "progress"
        })))
        .unwrap();
        let diagnostic =
            serde_json::to_value(SessionEvent::Diagnostic("line".to_string())).unwrap();

        assert_eq!(event["kind"], "event");
        assert_eq!(diagnostic["kind"], "diagnostic");
    }
}

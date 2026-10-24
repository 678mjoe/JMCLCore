//! Managed server remote adapters. Hostnames are never accepted: both transports
//! are loopback-only and ports come exclusively from the checked properties.
use crate::protocol::RpcError;
use serde_json::{Value, value::RawValue};
fn error(name: &str) -> RpcError {
    let (code, message) = match name {
        "ServerNotRunning" => ("SERVER_NOT_RUNNING", "The server is not running"),
        "ServerPropertiesInvalid" => ("SERVER_PROPERTIES_INVALID", "Server properties are invalid"),
        "ServerRconDisabled" => (
            "SERVER_RCON_DISABLED",
            "RCON is not enabled for this server",
        ),
        "ServerRconAuthenticationFailed" => {
            ("SERVER_RCON_AUTH_FAILED", "RCON authentication failed")
        }
        "ServerRconUnavailable" => ("SERVER_RCON_UNAVAILABLE", "RCON is unavailable"),
        "ServerRconProtocolError" => (
            "SERVER_RCON_PROTOCOL_ERROR",
            "RCON returned an invalid response",
        ),
        "ServerQueryDisabled" => (
            "SERVER_QUERY_DISABLED",
            "Query is not enabled for this server",
        ),
        "ServerQueryUnavailable" => ("SERVER_QUERY_UNAVAILABLE", "Query is unavailable"),
        "ServerQueryProtocolError" => (
            "SERVER_QUERY_PROTOCOL_ERROR",
            "Query returned an invalid response",
        ),
        "ServerNotFound" => ("SERVER_NOT_FOUND", "No server matches the requested id"),
        "InvalidServerRemoteInput" => (
            "INVALID_PARAMS",
            "server RCON command or password is invalid",
        ),
        "UnsupportedPlatform" => (
            "UNSUPPORTED_PLATFORM",
            "Server management is currently supported only on Linux",
        ),
        _ => ("SERVER_REMOTE_FAILED", "The server remote operation failed"),
    };
    RpcError::new(code, message)
}
pub fn execute_rcon(
    directory: &str,
    id: &str,
    password: &str,
    command: &str,
    max_bytes: usize,
) -> Result<super::rcon::Reply, &'static str> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, id, password, command, max_bytes);
        Err("UnsupportedPlatform")
    }
    #[cfg(target_os = "linux")]
    {
        let properties = live_properties(directory, id, true)?;
        let port = properties
            .rcon_port()
            .map_err(|_| "ServerPropertiesInvalid")?
            .ok_or("ServerRconDisabled")?;
        super::rcon::execute(port, password, command, max_bytes).map_err(|e| match e {
            "AuthenticationFailed" => "ServerRconAuthenticationFailed",
            "TransportUnavailable" => "ServerRconUnavailable",
            "ProtocolError" => "ServerRconProtocolError",
            _ => "InvalidServerRemoteInput",
        })
    }
}
pub fn execute_query(
    directory: &str,
    id: &str,
    mode: super::query::Mode,
) -> Result<super::query::Reply, &'static str> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (directory, id, mode);
        Err("UnsupportedPlatform")
    }
    #[cfg(target_os = "linux")]
    {
        let properties = live_properties(directory, id, false)?;
        let port = properties
            .query_port()
            .map_err(|_| "ServerPropertiesInvalid")?
            .ok_or("ServerQueryDisabled")?;
        super::query::execute(port, mode).map_err(|e| {
            if e == "ProtocolError" {
                "ServerQueryProtocolError"
            } else {
                "ServerQueryUnavailable"
            }
        })
    }
}
#[cfg(target_os = "linux")]
fn live_properties(
    directory: &str,
    id: &str,
    rcon: bool,
) -> Result<super::properties::Document, &'static str> {
    let root = super::store::open_root(directory).map_err(|_| "ServerNotFound")?;
    super::store::load(&root, id).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
            "ServerNotFound"
        } else {
            "ServerPropertiesInvalid"
        }
    })?;
    let dir = super::store::open_server(&root, id).map_err(|_| "ServerNotFound")?;
    if !super::lifecycle::status(&dir)
        .map_err(|_| {
            if rcon {
                "ServerRconUnavailable"
            } else {
                "ServerQueryUnavailable"
            }
        })?
        .running
    {
        return Err("ServerNotRunning");
    }
    let bytes =
        super::lifecycle::read(&dir, "server.properties", super::properties::MAX_FILE_BYTES)
            .map_err(|_| "ServerPropertiesInvalid")?;
    super::properties::parse(&bytes).map_err(|_| "ServerPropertiesInvalid")
}
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    if !matches!(method, "server.rcon.command" | "server.query") {
        return None;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = params;
        Some(Err(error("UnsupportedPlatform")))
    }
    #[cfg(target_os = "linux")]
    {
        Some(linux::handle(method, params))
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use serde::Deserialize;
    use zeroize::Zeroize;
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Secret(String);
    impl Drop for Secret {
        fn drop(&mut self) {
            self.0.zeroize();
        }
    }
    fn limit() -> u64 {
        super::super::rcon::MAX_REPLY as u64
    }
    fn full() -> String {
        "full".into()
    }
    #[derive(Deserialize)]
    struct RconParams {
        directory: String,
        id: String,
        password: Secret,
        command: Secret,
        #[serde(
            default = "limit",
            deserialize_with = "super::super::numbers::u64_number"
        )]
        max_bytes: u64,
    }
    #[derive(Deserialize)]
    struct QueryParams {
        directory: String,
        id: String,
        #[serde(default = "full")]
        mode: String,
    }
    fn invalid(message: &str) -> RpcError {
        RpcError::new("INVALID_PARAMS", message)
    }
    fn parse<T: serde::de::DeserializeOwned>(raw: &RawValue) -> Result<T, ()> {
        if !raw.get().trim_start().starts_with('{') {
            return Err(());
        }
        serde_json::from_str(raw.get()).map_err(|_| ())
    }
    pub fn handle(method: &str, params: Option<&RawValue>) -> Result<Value, RpcError> {
        if method == "server.rcon.command" {
            let raw = params.ok_or_else(|| {
                invalid("server RCON requires directory, id, password, and command")
            })?;
            let p: RconParams =
                parse(raw).map_err(|_| invalid("server RCON parameters are invalid"))?;
            if p.directory.is_empty()
                || p.id.is_empty()
                || p.password.0.is_empty()
                || p.password.0.len() > 4095
                || p.password.0.contains('\0')
                || p.command.0.is_empty()
                || p.command.0.len() > 4095
                || p.command.0.contains('\0')
                || p.max_bytes == 0
                || p.max_bytes > super::super::rcon::MAX_REPLY as u64
            {
                return Err(invalid("server RCON parameters are out of range"));
            }
            crate::instances::validate_id(&p.id)
                .map_err(|_| invalid("the server id is invalid"))?;
            let r = execute_rcon(
                &p.directory,
                &p.id,
                &p.password.0,
                &p.command.0,
                p.max_bytes as usize,
            )
            .map_err(error)?;
            Ok(serde_json::json!({"stage":method,"text":r.text,"truncated":r.truncated}))
        } else {
            let raw = params.ok_or_else(|| invalid("server query requires directory and id"))?;
            let p: QueryParams =
                parse(raw).map_err(|_| invalid("server query parameters are invalid"))?;
            if p.directory.is_empty() || p.id.is_empty() {
                return Err(invalid("server query parameters are out of range"));
            }
            crate::instances::validate_id(&p.id)
                .map_err(|_| invalid("the server id is invalid"))?;
            let mode = match p.mode.as_str() {
                "basic" => super::super::query::Mode::Basic,
                "full" => super::super::query::Mode::Full,
                _ => return Err(invalid("server query mode must be basic or full")),
            };
            let r = execute_query(&p.directory, &p.id, mode).map_err(error)?;
            Ok(query_value(&r))
        }
    }
}
pub(super) fn query_value(r: &super::query::Reply) -> Value {
    let mut value = serde_json::json!({"stage":"server.query"});
    value.as_object_mut().unwrap().extend(
        serde_json::to_value(r)
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
    );
    value
}

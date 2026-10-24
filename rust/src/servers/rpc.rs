use crate::protocol::RpcError;
use serde_json::{Value, value::RawValue};
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    #[cfg(target_os = "linux")]
    if let Some(result) = crate::worlds::server::handle(method, params) {
        return Some(result);
    }
    #[cfg(not(target_os = "linux"))]
    if matches!(
        method,
        "server.worlds.list"
            | "server.worlds.get"
            | "server.worlds.rename"
            | "server.worlds.delete"
            | "server.worlds.backup"
            | "server.worlds.backups"
            | "server.worlds.restore"
            | "server.worlds.backups.delete"
    ) {
        return Some(Err(RpcError::new(
            "UNSUPPORTED_PLATFORM",
            "Server management is currently supported only on Linux",
        )));
    }
    if !matches!(
        method,
        "server.get"
            | "server.list"
            | "server.delete"
            | "server.properties.get"
            | "server.properties.set"
            | "server.status"
            | "server.logs"
            | "server.command"
            | "server.stop"
            | "server.start"
            | "server.restart"
    ) {
        return None;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = params;
        Some(Err(RpcError::new(
            "UNSUPPORTED_PLATFORM",
            "Server management is currently supported only on Linux",
        )))
    }
    #[cfg(target_os = "linux")]
    {
        Some(linux::handle(method, params))
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::servers::{lifecycle as lc, properties, store};
    use rustix::fs::{self, AtFlags, OFlags};
    use serde::Deserialize;
    use std::{fs::File, io};
    fn err(c: &str, m: &str) -> RpcError {
        RpcError::new(c, m)
    }
    fn invalid(m: impl AsRef<str>) -> RpcError {
        err("INVALID_PARAMS", m.as_ref())
    }
    fn grace() -> u32 {
        10000
    }
    fn maxbytes() -> u64 {
        65536
    }
    #[derive(Deserialize)]
    struct Params {
        directory: String,
        #[serde(default)]
        id: String,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        properties: Option<Value>,
        #[serde(default)]
        java_paths: Vec<String>,
        #[serde(
            default = "grace",
            deserialize_with = "crate::instances::schema_number"
        )]
        grace_ms: u32,
        #[serde(
            default = "maxbytes",
            deserialize_with = "crate::servers::numbers::u64_number"
        )]
        max_bytes: u64,
        #[serde(default, deserialize_with = "optional_number")]
        cursor: Option<u64>,
        #[serde(default, deserialize_with = "optional_number")]
        file_id: Option<u64>,
    }
    fn optional_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        let value = Value::deserialize(d)?;
        if value.is_null() {
            return Ok(None);
        }
        let result = crate::servers::numbers::u64_value(&value);
        result
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("Expected u64"))
    }
    fn wording(method: &str) -> (&'static str, &'static str) {
        match method {
            "server.get" => ("lookup", "a directory and an id"),
            "server.list" => ("listing", "a directory"),
            "server.delete" => ("deletion", "a directory and an id"),
            "server.properties.get" => ("properties lookup", "directory and id"),
            "server.properties.set" => ("properties update", "directory, id, and properties"),
            "server.command" => ("command", "directory, id, and command"),
            "server.logs" => ("logs", "directory and id"),
            "server.stop" => ("stop", "directory and id"),
            "server.start" => ("start", "directory and id"),
            "server.restart" => ("restart", "directory and id"),
            _ => ("status", "directory and id"),
        }
    }
    fn load(root: &File, id: &str) -> Result<crate::servers::manifest::Manifest, RpcError> {
        store::load(root, id).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
                err("SERVER_NOT_FOUND", "No server matches the given id")
            } else {
                err("SERVER_LOAD_FAILED", "Failed to load the server")
            }
        })
    }
    fn properties(dir: &File) -> Result<properties::Document, RpcError> {
        let b =
            lc::read(dir, "server.properties", properties::MAX_FILE_BYTES).map_err(|e| match e
                .kind()
            {
                io::ErrorKind::NotFound => err(
                    "SERVER_PROPERTIES_NOT_FOUND",
                    "server.properties has not been created",
                ),
                io::ErrorKind::InvalidData => err(
                    "SERVER_PROPERTIES_INVALID",
                    "server.properties is malformed or out of range",
                ),
                _ => err(
                    "SERVER_PROPERTIES_READ_FAILED",
                    "Failed to read server.properties",
                ),
            })?;
        properties::parse(&b).map_err(|_| {
            err(
                "SERVER_PROPERTIES_INVALID",
                "server.properties is malformed or out of range",
            )
        })
    }
    fn property_value(d: &properties::Document) -> Value {
        let mut v = serde_json::Map::new();
        for e in &d.entries {
            v.insert(
                e.key.clone(),
                Value::String(if e.key == "rcon.password" {
                    "<redacted>".into()
                } else {
                    e.value.clone()
                }),
            );
        }
        Value::Object(v)
    }
    fn manifest(m: &crate::servers::manifest::Manifest, installed: bool) -> Value {
        serde_json::json!({"id":m.id,"name":m.name,"version_id":m.version_id,"fabric_loader":m.fabric_loader,"neoforge_version":m.neoforge_version,"forge_version":m.forge_version,"source":m.source,"accept_eula":m.accept_eula,"java_path":m.java_path,"installed":installed})
    }
    fn installed(dir: &File, m: &crate::servers::manifest::Manifest) -> Result<bool, RpcError> {
        store::installed_java_major(dir, m)
            .map(|m| m.is_some())
            .map_err(|_| {
                err(
                    "SERVER_STATUS_FAILED",
                    "Failed to read server installation status",
                )
            })
    }
    fn remove_tree(parent: &File, name: &std::ffi::CStr) -> io::Result<()> {
        let dir = match fs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            fs::Mode::empty(),
        ) {
            Ok(d) => File::from(d),
            Err(rustix::io::Errno::NOTDIR | rustix::io::Errno::LOOP) => {
                return fs::unlinkat(parent, name, AtFlags::empty()).map_err(Into::into);
            }
            Err(e) => return Err(e.into()),
        };
        for entry in fs::Dir::read_from(&dir)? {
            let entry = entry?;
            if matches!(entry.file_name().to_bytes(), b"." | b"..") {
                continue;
            }
            remove_tree(&dir, entry.file_name())?;
        }
        fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(Into::into)
    }
    pub fn handle(method: &str, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let (word, requires) = wording(method);
        let raw = raw.ok_or_else(|| invalid(format!("server {word} requires {requires}")))?;
        let value: Value = serde_json::from_str(raw.get())
            .map_err(|_| invalid(format!("server {word} parameters are invalid")))?;
        let mut selected = value
            .as_object()
            .cloned()
            .ok_or_else(|| invalid(format!("server {word} parameters are invalid")))?;
        selected.retain(|key, _| {
            key == "directory"
                || (key == "id" && method != "server.list")
                || matches!(
                    (method, key.as_str()),
                    ("server.logs", "cursor" | "file_id" | "max_bytes")
                        | ("server.stop" | "server.restart", "grace_ms")
                        | ("server.start" | "server.restart", "java_paths")
                        | ("server.command", "command")
                        | ("server.properties.set", "properties")
                )
        });
        let p: Params = serde_json::from_value(Value::Object(selected))
            .map_err(|_| invalid(format!("server {word} parameters are invalid")))?;
        if (method != "server.list" && !value.get("id").is_some_and(Value::is_string))
            || (method == "server.command" && p.command.is_none())
            || (method == "server.properties.set"
                && !value.as_object().unwrap().contains_key("properties"))
        {
            return Err(invalid(format!("server {word} parameters are invalid")));
        }
        let lifecycle = matches!(
            method,
            "server.start"
                | "server.restart"
                | "server.stop"
                | "server.status"
                | "server.logs"
                | "server.command"
        );
        if p.directory.is_empty() || (method != "server.list" && p.id.is_empty()) {
            return Err(invalid(if lifecycle {
                "server directory and id must not be empty".into()
            } else if method == "server.list" {
                "server listing directory must not be empty".into()
            } else {
                format!("server {word} parameters are out of range")
            }));
        }
        if lifecycle && crate::instances::validate_id(&p.id).is_err() {
            return Err(invalid("the server id is invalid"));
        }
        if matches!(method, "server.stop" | "server.restart")
            && p.grace_ms as u64 > lc::MAX_GRACE_MS
        {
            return Err(invalid("server grace_ms is out of range"));
        }
        if method == "server.logs"
            && (p.max_bytes == 0 || p.max_bytes > lc::MAX_LOG_READ_BYTES as u64)
        {
            return Err(invalid("server max_bytes is out of range"));
        }
        if method == "server.command" {
            let command = p.command.as_deref().unwrap();
            if command.is_empty()
                || command.len() > 4095
                || command.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r'))
            {
                return Err(invalid(
                    "server command must be a nonempty single line within the size limit",
                ));
            }
        }
        if method == "server.properties.set"
            && !p
                .properties
                .as_ref()
                .and_then(Value::as_object)
                .is_some_and(|o| !o.is_empty() && o.len() <= 1024)
        {
            return Err(invalid(
                "server properties update parameters are out of range",
            ));
        }
        let root = store::open_root(&p.directory)
            .map_err(|_| err("SERVERS_ROOT_ERROR", "Failed to open the servers root"))?;
        let mut out = serde_json::json!({"stage":method});
        if method == "server.list" {
            let entries = fs::Dir::read_from(&root)
                .map_err(|_| err("SERVER_LIST_FAILED", "Failed to list servers"))?;
            let mut docs = Vec::new();
            for entry in entries {
                let entry =
                    entry.map_err(|_| err("SERVER_LIST_FAILED", "Failed to list servers"))?;
                let Ok(name) = entry.file_name().to_str() else {
                    continue;
                };
                if crate::instances::validate_id(name).is_err()
                    || !matches!(
                        entry.file_type(),
                        fs::FileType::Directory | fs::FileType::Unknown
                    )
                {
                    continue;
                }
                if docs.len() == 4096 {
                    return Err(err("SERVER_LIST_FAILED", "Failed to list servers"));
                }
                if let Ok(m) = store::load(&root, name) {
                    docs.push(m);
                }
            }
            docs.sort_by(|a, b| a.id.cmp(&b.id));
            let mut servers = Vec::new();
            for m in docs {
                let dir = store::open_server(&root, &m.id).map_err(|_| {
                    err(
                        "SERVER_STATUS_FAILED",
                        "Failed to read server installation status",
                    )
                })?;
                servers.push(manifest(&m, installed(&dir, &m)?));
            }
            out["servers"] = Value::Array(servers);
            return Ok(out);
        }
        let m = load(&root, &p.id)?;
        let dir = store::open_server(&root, &p.id)
            .map_err(|_| err("SERVER_ROOT_ERROR", "Failed to open the server directory"))?;
        out["id"] = Value::String(p.id.clone());
        match method {
            "server.get" => {
                out.as_object_mut().unwrap().extend(
                    manifest(&m, installed(&dir, &m)?)
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            }
            "server.delete" => {
                let _guard = lc::lock_stopped(&dir).map_err(|e| {
                    if e == "ServerAlreadyRunning" {
                        err(
                            "SERVER_ALREADY_RUNNING",
                            "Stop the server before deleting it",
                        )
                    } else {
                        err(
                            "SERVER_STATUS_FAILED",
                            "Failed to validate the server lifecycle status",
                        )
                    }
                })?;
                let name = std::ffi::CString::new(p.id).unwrap();
                remove_tree(&root, &name)
                    .map_err(|_| err("SERVER_DELETE_FAILED", "Failed to delete the server"))?;
                out["deleted"] = Value::Bool(true);
            }
            "server.properties.get" | "server.properties.set" => {
                let _guard = if method.ends_with(".set") {
                    Some(lc::lock_stopped(&dir).map_err(|e| {
                        if e == "ServerAlreadyRunning" {
                            err(
                                "SERVER_ALREADY_RUNNING",
                                "Stop the server before changing its properties",
                            )
                        } else {
                            err(
                                "SERVER_STATUS_FAILED",
                                "Failed to validate the server lifecycle state",
                            )
                        }
                    })?)
                } else {
                    None
                };
                let mut d = properties(&dir)?;
                if let Some(props) = p.properties.filter(|_| method.ends_with(".set")) {
                    for (key, v) in props.as_object().unwrap() {
                        let v = v
                            .as_str()
                            .ok_or_else(|| invalid("server property values must be strings"))?;
                        d.set(key, v).map_err(|_| {
                            err(
                                "SERVER_PROPERTIES_INVALID",
                                "A server property key or value is out of range",
                            )
                        })?;
                    }
                    lc::atomic_write(&dir, "server.properties", &d.encode()).map_err(|_| {
                        err(
                            "SERVER_PROPERTIES_WRITE_FAILED",
                            "Failed to atomically update server.properties",
                        )
                    })?;
                }
                out["properties"] = property_value(&d);
            }
            "server.status" => {
                let status = lc::status(&dir).map_err(|_| {
                    err(
                        "SERVER_STATUS_FAILED",
                        "Failed to read the server lifecycle status",
                    )
                })?;
                out.as_object_mut().unwrap().extend(
                    serde_json::to_value(status)
                        .unwrap()
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            }
            "server.logs" => {
                let logs = lc::read_logs(&dir, p.cursor, p.file_id, p.max_bytes as usize).map_err(
                    |e| {
                        if e == "ServerLogsNotFound" {
                            err("SERVER_LOGS_NOT_FOUND", "No server console log exists")
                        } else {
                            err(
                                "SERVER_LOGS_FAILED",
                                "Failed to read the server console log",
                            )
                        }
                    },
                )?;
                out.as_object_mut().unwrap().extend(
                    serde_json::to_value(logs)
                        .unwrap()
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            }
            "server.command" => {
                let pid = lc::command(&dir, p.command.as_deref().unwrap()).map_err(|e| {
                    if e == "ServerNotRunning" {
                        err("SERVER_NOT_RUNNING", "The server is not running")
                    } else {
                        err("SERVER_COMMAND_FAILED", "Failed to send the server command")
                    }
                })?;
                out["accepted"] = Value::Bool(true);
                out["pid"] = pid.into();
            }
            "server.stop" => {
                let stop = lc::stop(&dir, p.grace_ms.into())
                    .map_err(|_| err("SERVER_STOP_FAILED", "Failed to stop the server"))?;
                if stop.termination == lc::Termination::NotRunning {
                    return Err(err("SERVER_NOT_RUNNING", "The server is not running"));
                }
                out["running"] = false.into();
                out["termination"] = serde_json::to_value(stop.termination).unwrap();
                out["pid"] = stop.pid.unwrap().into();
            }
            "server.start" | "server.restart" => {
                let failure = if method == "server.start" {
                    ("SERVER_START_FAILED", "Failed to start the server")
                } else {
                    ("SERVER_RESTART_FAILED", "Failed to restart the server")
                };
                if method == "server.start"
                    && lc::status(&dir)
                        .map_err(|_| {
                            err(
                                "SERVER_STATUS_FAILED",
                                "Failed to validate the server lifecycle status",
                            )
                        })?
                        .running
                {
                    return Err(err(
                        "SERVER_ALREADY_RUNNING",
                        "The server is already running",
                    ));
                }
                let major = store::installed_java_major(&dir, &m)
                    .map_err(|_| {
                        err(
                            failure.0,
                            "Failed to read the completed server installation",
                        )
                    })?
                    .ok_or_else(|| {
                        err(
                            "SERVER_NOT_INSTALLED",
                            "The server installation is incomplete or does not match its manifest",
                        )
                    })?;
                let paths = m.java_path.map(|p| vec![p]).unwrap_or(p.java_paths);
                let env = std::env::vars_os().collect();
                let detection = crate::java::detect::detect(&env, &paths).map_err(|_| {
                    err("JAVA_NOT_FOUND", "No Java executable candidates were found")
                })?;
                let arch = crate::java::managed::host_arch().ok_or_else(|| {
                    err("UNSUPPORTED_PLATFORM", "The host platform is not supported")
                })?;
                let runtime = crate::java::detect::select_compatible(&detection, major, arch)
                    .map_err(|_| {
                        err(
                            "JAVA_INCOMPATIBLE",
                            "No Java runtime matches the installed server requirement",
                        )
                    })?;
                let java = std::fs::canonicalize(runtime.executable).map_err(|_| {
                    err(
                        "JAVA_NOT_FOUND",
                        "The selected Java executable could not be resolved",
                    )
                })?;
                let java = java.to_str().ok_or_else(|| {
                    err(
                        "JAVA_NOT_FOUND",
                        "The selected Java executable could not be resolved",
                    )
                })?;
                let result = if method == "server.start" {
                    lc::start(&dir, java)
                } else {
                    lc::restart(&dir, java, p.grace_ms.into())
                }
                .map_err(|e| {
                    if e == "ServerAlreadyRunning" {
                        err("SERVER_ALREADY_RUNNING", "The server is already running")
                    } else {
                        err(failure.0, failure.1)
                    }
                })?;
                out["running"] = true.into();
                out["pid"] = result.pid.into();
                out["supervisor_pid"] = result.supervisor_pid.into();
                out["started_at_ms"] = result.started_at_ms.into();
                out["java_path"] = java.into();
            }
            _ => unreachable!(),
        }
        Ok(out)
    }
}

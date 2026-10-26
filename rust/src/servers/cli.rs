use serde_json::{Value, json};
use std::io::{self, Write};
const TOP: &str = "usage: jmcl-core server <create|list|show|delete|install|properties|worlds|mods|datapacks|start|stop|restart|status|logs|command|rcon|query> [options]";
fn usage(command: &str) -> String {
    let tail = match command {
        "list" => "[--servers-directory PATH] [--json]",
        "show" | "status" => "ID [--servers-directory PATH] [--json]",
        "delete" => "ID [--servers-directory PATH]",
        "start" => "ID [--servers-directory PATH] [--java-path PATH ...]",
        "stop" => "ID [--grace-ms N] [--servers-directory PATH]",
        "restart" => "ID [--grace-ms N] [--servers-directory PATH] [--java-path PATH ...]",
        "logs" => {
            "ID [--cursor N] [--file-id N] [--max-bytes N] [--servers-directory PATH] [--json]"
        }
        "command" => "ID COMMAND [--servers-directory PATH]",
        "properties get" => "ID [--servers-directory PATH] [--json]",
        "properties set" => "ID key=value ... [--servers-directory PATH]",
        _ => return TOP.into(),
    };
    format!("usage: jmcl-core server {command} {tail}")
}
fn options(command: &str, args: &[String]) -> Option<(Value, bool)> {
    let mut p = json!({"directory":"servers"});
    let mut i = 0;
    let mut json_output = false;
    if command != "list" {
        let id = args.first()?;
        crate::instances::validate_id(id).ok()?;
        p["id"] = id.clone().into();
        i += 1;
    }
    if command == "command" {
        let c = args.get(i)?;
        if c.is_empty() || c.len() > 4095 || c.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r')) {
            return None;
        }
        p["command"] = c.clone().into();
        i += 1;
    }
    let mut pairs = serde_json::Map::new();
    let mut paths = Vec::new();
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--json"
            && matches!(
                command,
                "list" | "show" | "status" | "logs" | "properties get"
            )
        {
            json_output = true;
            i += 1;
            continue;
        }
        if command == "properties set" && arg != "--servers-directory" {
            let (k, v) = arg.split_once('=')?;
            if k.is_empty() {
                return None;
            }
            pairs.insert(k.into(), v.into());
            i += 1;
            continue;
        }
        let value = args.get(i + 1)?;
        if value.is_empty() {
            return None;
        }
        match arg {
            "--servers-directory" => p["directory"] = value.clone().into(),
            "--java-path" if matches!(command, "start" | "restart") => paths.push(value.clone()),
            "--grace-ms" if matches!(command, "stop" | "restart") => {
                let n: u32 = value.parse().ok()?;
                if n > 120000 {
                    return None;
                }
                p["grace_ms"] = n.into();
            }
            "--cursor" | "--file-id" | "--max-bytes" if command == "logs" => {
                let n: u64 = value.parse().ok()?;
                if arg == "--max-bytes" && (n == 0 || n > 1024 * 1024) {
                    return None;
                }
                p[&arg[2..].replace('-', "_")] = n.into();
            }
            _ => return None,
        }
        i += 2;
    }
    if matches!(command, "start" | "restart") {
        p["java_paths"] = json!(paths);
    }
    if command == "properties set" {
        if pairs.is_empty() {
            return None;
        }
        p["properties"] = Value::Object(pairs);
    }
    Some((p, json_output))
}
fn text(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    if let Some(result) = crate::server_content::cli(args, out, err) {
        return result;
    }
    if args.first().is_some_and(|a| a == "install") {
        return super::installation_cli::cli(&args[1..], out, err);
    }
    if args.first().is_some_and(|a| a == "create") {
        return super::creation_cli::cli(&args[1..], out, err);
    }
    if let Some(kind) = args
        .first()
        .filter(|a| matches!(a.as_str(), "rcon" | "query"))
    {
        return super::remote_cli::cli(kind, &args[1..], out, err);
    }
    if args.first().is_some_and(|a| a == "worlds") {
        return crate::worlds::server_cli::cli(&args[1..], out, err);
    }
    let mut command = args.first().map(String::as_str).unwrap_or("").to_owned();
    let mut offset = 1;
    if command == "properties" {
        match args.get(1).map(String::as_str) {
            Some("get" | "set") => {
                command.push(' ');
                command.push_str(&args[1]);
                offset = 2;
            }
            _ => {
                writeln!(
                    err,
                    "usage: jmcl-core server properties <get|set> ID [options]"
                )?;
                return Ok(2);
            }
        }
    }
    if !matches!(
        command.as_str(),
        "list"
            | "show"
            | "delete"
            | "start"
            | "stop"
            | "restart"
            | "status"
            | "logs"
            | "command"
            | "properties get"
            | "properties set"
    ) {
        writeln!(err, "{TOP}")?;
        return Ok(2);
    }
    if !cfg!(target_os = "linux") {
        writeln!(
            err,
            "Server management is currently supported only on Linux"
        )?;
        return Ok(1);
    }
    let Some((params, json_output)) = options(&command, &args[offset..]) else {
        writeln!(err, "{}", usage(&command))?;
        return Ok(2);
    };
    let method = format!(
        "server.{}",
        if command == "show" {
            "get".into()
        } else {
            command.replace(' ', ".")
        }
    );
    let raw = serde_json::value::to_raw_value(&params)?;
    #[cfg(target_os = "linux")]
    let (response, load_diagnostic) = if command == "show" {
        super::rpc::show_with_load_diagnostic(Some(&raw))
    } else if command == "delete" {
        super::rpc::delete_with_diagnostic(Some(&raw))
    } else {
        (super::handle(&method, Some(&raw)).unwrap(), None)
    };
    #[cfg(not(target_os = "linux"))]
    let (response, load_diagnostic): (_, Option<String>) =
        (super::handle(&method, Some(&raw)).unwrap(), None);
    let mut result = match response {
        Ok(v) => v,
        Err(e) => {
            if let Some(message) = load_diagnostic {
                writeln!(err, "{message}")?;
                return Ok(1);
            }
            let message = match e.code.as_str() {
                "SERVER_NOT_FOUND" if command == "show" => {
                    "failed to load server: FileNotFound".into()
                }
                "SERVER_NOT_FOUND" if command == "delete" => {
                    "failed to open server directory: FileNotFound".into()
                }
                "SERVER_NOT_FOUND" => "no server matches the given id".into(),
                "SERVER_COMMAND_FAILED" => {
                    "failed to send the server command: ServerCommandFailed".into()
                }
                "SERVER_LOGS_FAILED" => {
                    "failed to read the server console log: ServerLogsFailed".into()
                }
                "SERVER_STOP_FAILED" => "failed to stop the server: ServerStopFailed".into(),
                "SERVER_STATUS_FAILED" if command == "status" => {
                    "failed to read the server lifecycle status: ServerStatusFailed".into()
                }
                _ => {
                    let mut chars = e.message.chars();
                    match chars.next() {
                        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
                        None => String::new(),
                    }
                }
            };
            writeln!(err, "{message}")?;
            return Ok(1);
        }
    };
    if json_output {
        if matches!(command.as_str(), "list" | "show" | "properties get") {
            result.as_object_mut().unwrap().shift_remove("stage");
        }
        if command == "show" {
            result = json!({"server":result});
        }
        serde_json::to_writer(&mut *out, &result)?;
        writeln!(out)?;
        return Ok(0);
    }
    let id = result.get("id").map(text).unwrap_or_default();
    match command.as_str() {
        "list" => {
            for s in result["servers"].as_array().unwrap() {
                write!(
                    out,
                    "{}\t{}\t{}\t{}\t",
                    text(&s["id"]),
                    text(&s["version_id"]),
                    text(&s["source"]),
                    if s["installed"] == true {
                        "installed"
                    } else {
                        "not-installed"
                    },
                )?;
                write_name(out, &s["name"])?;
                out.write_all(b"\n")?;
            }
        }
        "show" => {
            write!(out, "server\t{id}\nname\t")?;
            write_name(out, &result["name"])?;
            writeln!(
                out,
                "\nversion\t{}\nsource\t{}\ninstalled\t{}",
                text(&result["version_id"]),
                text(&result["source"]),
                text(&result["installed"])
            )?;
            for (key, label) in [
                ("fabric_loader", "fabric-loader"),
                ("neoforge_version", "neoforge-version"),
                ("forge_version", "forge-version"),
            ] {
                if !result[key].is_null() {
                    writeln!(out, "{label}\t{}", text(&result[key]))?;
                }
            }
        }
        "delete" => writeln!(out, "deleted\t{id}")?,
        "properties get" | "properties set" => {
            for (k, v) in result["properties"].as_object().unwrap() {
                writeln!(out, "{k}={}", text(v))?;
            }
        }
        "logs" => out.write_all(result["text"].as_str().unwrap().as_bytes())?,
        "status" => {
            writeln!(
                out,
                "server\t{id}\nrunning\t{}\nstaleState\t{}",
                text(&result["running"]),
                text(&result["stale_state"])
            )?;
            for (key, label) in [
                ("pid", "pid"),
                ("supervisor_pid", "supervisorPid"),
                ("java_path", "java"),
                ("started_at_ms", "startedAtMs"),
                ("uptime_ms", "uptimeMs"),
            ] {
                if !result[key].is_null() {
                    writeln!(out, "{label}\t{}", text(&result[key]))?;
                }
            }
        }
        "start" | "restart" => writeln!(
            out,
            "server\t{id}\nrunning\ttrue\npid\t{}\nsupervisorPid\t{}\nstartedAtMs\t{}\njava\t{}",
            text(&result["pid"]),
            text(&result["supervisor_pid"]),
            text(&result["started_at_ms"]),
            text(&result["java_path"])
        )?,
        "stop" => writeln!(
            out,
            "server\t{id}\nrunning\tfalse\ntermination\t{}",
            text(&result["termination"])
        )?,
        "command" => writeln!(
            out,
            "server\t{id}\naccepted\ttrue\npid\t{}",
            text(&result["pid"])
        )?,
        _ => unreachable!(),
    }
    Ok(0)
}

fn write_name(out: &mut impl std::io::Write, name: &serde_json::Value) -> std::io::Result<()> {
    if let Some(text) = name.as_str() {
        out.write_all(text.as_bytes())
    } else {
        let bytes: Vec<u8> = serde_json::from_value(name.clone())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        out.write_all(&bytes)
    }
}

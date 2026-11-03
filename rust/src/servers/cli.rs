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
struct Options {
    params: Value,
    json_output: bool,
    property_pairs: Vec<super::properties::Entry>,
}
fn options(command: &str, args: &[String]) -> Option<Options> {
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
    let mut property_pairs = Vec::new();
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
            if arg.starts_with("--") || property_pairs.len() == super::properties::MAX_ENTRIES {
                return None;
            }
            let (k, v) = arg.split_once('=')?;
            if k.is_empty() {
                return None;
            }
            property_pairs.push(super::properties::Entry {
                key: k.into(),
                value: v.into(),
            });
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
                let n = crate::cli_number::unsigned(value)?;
                if n > 120000 {
                    return None;
                }
                p["grace_ms"] = n.into();
            }
            "--cursor" | "--file-id" | "--max-bytes" if command == "logs" => {
                let n = crate::cli_number::unsigned(value)?;
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
    Some(Options {
        params: p,
        json_output,
        property_pairs,
    })
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
    let Some(Options {
        params,
        json_output,
        property_pairs,
    }) = options(&command, &args[offset..])
    else {
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
    #[cfg(not(target_os = "linux"))]
    let _ = &property_pairs;
    #[cfg(target_os = "linux")]
    let (response, load_diagnostic) = if command == "show" {
        super::rpc::show_with_load_diagnostic(Some(&raw))
    } else if command == "delete" {
        super::rpc::delete_with_diagnostic(Some(&raw))
    } else if command == "properties set" {
        (
            super::rpc::properties_set_cli(Some(&raw), &property_pairs),
            None,
        )
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

#[cfg(test)]
mod cli_number_tests {
    use super::*;
    #[test]
    fn pure_server_numbers_keep_zig_grammar_and_ranges() {
        for (command, flag, field, raw, n) in [
            ("stop", "--grace-ms", "grace_ms", "1_000", 1000),
            ("restart", "--grace-ms", "grace_ms", "-0", 0),
            ("stop", "--grace-ms", "grace_ms", "120_000", 120000),
            ("logs", "--cursor", "cursor", "1__0", 10),
            ("logs", "--file-id", "file_id", "-0", 0),
            ("logs", "--max-bytes", "max_bytes", "1_024", 1024),
            ("logs", "--max-bytes", "max_bytes", "1_048_576", 1048576),
            (
                "logs",
                "--cursor",
                "cursor",
                "18446744073709551615",
                u64::MAX,
            ),
        ] {
            let Options { params: p, .. } =
                options(command, &["demo".into(), flag.into(), raw.into()]).unwrap();
            assert_eq!(p[field], n);
        }
        for (command, flag, raw) in [
            ("stop", "--grace-ms", "120001"),
            ("restart", "--grace-ms", "4294967296"),
            ("logs", "--max-bytes", "0"),
            ("logs", "--max-bytes", "1048577"),
            ("logs", "--cursor", "18446744073709551616"),
            ("logs", "--cursor", "+_1"),
            ("logs", "--file-id", "-+0"),
            ("logs", "--cursor", "-1"),
        ] {
            assert!(options(command, &["demo".into(), flag.into(), raw.into()]).is_none());
        }
        assert!(
            options(
                "stop",
                &[
                    "demo".into(),
                    "--grace-ms".into(),
                    "120001".into(),
                    "--grace-ms".into(),
                    "0".into()
                ]
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod property_input_tests {
    use super::*;
    #[test]
    fn flag_shaped_assignments_and_raw_pair_overflow_are_rejected() {
        for raw in ["--bogus=value", "--json=true", "--servers-directory=x"] {
            assert!(options("properties set", &["demo".into(), raw.into()]).is_none());
        }
        let mut args = vec!["demo".into()];
        args.extend(std::iter::repeat_n("a=b".into(), 1024));
        assert!(options("properties set", &args).is_some());
        args.push("a=b".into());
        assert!(options("properties set", &args).is_none());
    }
    #[test]
    fn ordered_duplicate_values_are_validated_without_early_rejection() {
        use super::super::properties::{Document, MAX_VALUE_BYTES};
        let huge = "x".repeat(MAX_VALUE_BYTES + 1);
        for values in [[huge.as_str(), "ok"], ["ok", huge.as_str()]] {
            let o = options(
                "properties set",
                &[
                    "demo".into(),
                    format!("motd={}", values[0]),
                    format!("motd={}", values[1]),
                ],
            )
            .unwrap();
            assert_eq!(o.property_pairs.len(), 2);
            let mut d = Document::default();
            assert_eq!(
                d.set_ordered(&o.property_pairs).unwrap_err().0,
                "InvalidServerPropertyValue"
            );
        }
        let o = options(
            "properties set",
            &[
                "demo".into(),
                "z=first".into(),
                "a=second".into(),
                "z=last".into(),
            ],
        )
        .unwrap();
        let mut d = Document::default();
        d.set_ordered(&o.property_pairs).unwrap();
        assert_eq!(
            d.entries
                .iter()
                .map(|e| (e.key.as_str(), e.value.as_str()))
                .collect::<Vec<_>>(),
            vec![("z", "last"), ("a", "second")]
        );
    }
    #[test]
    fn directory_options_do_not_count_as_property_pairs() {
        let mut args = vec!["demo".into(), "--servers-directory".into(), "first".into()];
        args.extend(std::iter::repeat_n("a=b".into(), 1024));
        args.extend(["--servers-directory".into(), "last".into()]);
        let o = options("properties set", &args).unwrap();
        assert_eq!(o.property_pairs.len(), 1024);
        assert_eq!(o.params["directory"], "last");
        assert!(
            options(
                "properties set",
                &["demo".into(), "--servers-directory".into(), "root".into()]
            )
            .is_none()
        );
    }
}

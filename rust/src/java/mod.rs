//! Java discovery and existing managed-runtime receipt compatibility.
//! Network installation/selection is wired separately once metadata transport lands.
pub mod detect;
pub mod receipt;
pub mod store;
use crate::protocol::RpcError;
use serde::Deserialize;
use serde_json::{Value, value::RawValue};
use std::io::{self, Write};
#[derive(Default, Deserialize)]
struct DetectParams {
    #[serde(default)]
    paths: Vec<String>,
}
fn default_store() -> String {
    ".jmclcore".into()
}
#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_store")]
    store_directory: String,
}
#[derive(Deserialize)]
struct RemoveParams {
    runtime: String,
    #[serde(default = "default_store")]
    store_directory: String,
}
fn invalid(message: &str) -> RpcError {
    RpcError::new("INVALID_PARAMS", message)
}
fn runtime_error(e: &str) -> RpcError {
    match e {
        "InvalidInput"
        | "InvalidWorkerCount"
        | "InvalidRetryCount"
        | "InvalidRuntimeId"
        | "InvalidRuntimeName"
        | "InvalidRuntimePlatform" => invalid("Managed runtime parameters are invalid"),
        "RuntimeNotFound" => RpcError::new(
            "JAVA_RUNTIME_NOT_FOUND",
            "The managed runtime was not found",
        ),
        _ => RpcError::new("JAVA_RUNTIME_ERROR", "Managed runtime operation failed"),
    }
}
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    match method {
        "java.detect" => Some((|| {
            let p = match params {
                Some(p) => object_params::<DetectParams>(p)
                    .map_err(|_| invalid("java.detect paths must be strings"))?,
                None => DetectParams::default(),
            };
            let env = std::env::vars_os().collect();
            let d = detect::detect(&env, &p.paths).map_err(|e| {
                if e == "JavaNotFound" {
                    RpcError::new("JAVA_NOT_FOUND", "No Java executable candidates were found")
                } else {
                    RpcError::new(
                        "JAVA_DETECTION_FAILED",
                        "Failed to discover local Java runtimes",
                    )
                }
            })?;
            Ok(serde_json::to_value(d).expect("public Java metadata serializes"))
        })()),
        "java.runtime.list" => Some((|| {
            let p = match params {
                Some(p) => object_params::<ListParams>(p)
                    .map_err(|_| invalid("java.runtime.list parameters are invalid"))?,
                None => ListParams {
                    store_directory: default_store(),
                },
            };
            if p.store_directory.is_empty() {
                return Err(invalid("store directory must not be empty"));
            };
            let docs = store::list(&p.store_directory).map_err(runtime_error)?;
            Ok(
                serde_json::json!({"runtimes":docs.iter().map(|d|d.receipt.rpc(&d.name)).collect::<Vec<_>>()}),
            )
        })()),
        "java.runtime.remove" => Some((|| {
            let raw =
                params.ok_or_else(|| invalid("java.runtime.remove requires a runtime name"))?;
            let p: RemoveParams = object_params(raw)
                .map_err(|_| invalid("java.runtime.remove parameters are invalid"))?;
            if p.runtime.is_empty() || p.store_directory.is_empty() {
                return Err(invalid(
                    "runtime name and store directory must not be empty",
                ));
            };
            store::remove(&p.store_directory, &p.runtime).map_err(runtime_error)?;
            Ok(serde_json::json!({"removed":true,"runtime":p.runtime}))
        })()),
        _ => None,
    }
}
fn parse_store(args: &[String], allow_json: bool) -> Result<(String, bool), ()> {
    let mut store = default_store();
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--json" && allow_json {
            json = true;
            i += 1;
            continue;
        }
        if args[i] != "--store" || i + 1 >= args.len() || args[i + 1].is_empty() {
            return Err(());
        }
        store = args[i + 1].clone();
        i += 2;
    }
    Ok((store, json))
}
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    match args.first().map(String::as_str) {
        Some("detect") => {
            let mut paths = vec![];
            let mut json = false;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        json = true;
                        i += 1
                    }
                    "--path" if i + 1 < args.len() && !args[i + 1].is_empty() => {
                        paths.push(args[i + 1].clone());
                        i += 2
                    }
                    _ => {
                        writeln!(err, "usage: jmcl-core java detect [--path PATH] [--json]")?;
                        return Ok(2);
                    }
                }
            }
            let env = std::env::vars_os().collect();
            let d = match detect::detect(&env, &paths) {
                Ok(d) => d,
                Err(e) => {
                    writeln!(err, "Java detection failed: {e}")?;
                    return Ok(1);
                }
            };
            if json {
                serde_json::to_writer(&mut *out, &serde_json::json!({"protocol":1,"result":d}))?;
                writeln!(out)?;
                return Ok(0);
            }
            for r in &d.runtimes {
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}",
                    r.executable,
                    r.major_version,
                    r.os_arch,
                    r.vendor.as_deref().unwrap_or("unknown")
                )?
            }
            for f in d.failures {
                writeln!(
                    err,
                    "Java probe failed: {} ({})",
                    f.executable, f.error_name
                )?
            }
            Ok(if d.runtimes.is_empty() { 1 } else { 0 })
        }
        Some("runtime") => match args.get(1).map(String::as_str) {
            Some("list") => {
                let (store, json) = match parse_store(&args[2..], true) {
                    Ok(p) => p,
                    Err(()) => {
                        writeln!(
                            err,
                            "usage: jmcl-core java runtime list [--store DIR] [--json]"
                        )?;
                        return Ok(2);
                    }
                };
                let docs = match store::list(&store) {
                    Ok(d) => d,
                    Err(e) => {
                        writeln!(err, "failed to list managed runtimes: {e}")?;
                        return Ok(1);
                    }
                };
                if json {
                    let v = serde_json::json!({"protocol":1,"result":{"runtimes":docs.iter().map(|d|d.receipt.cli(&d.name)).collect::<Vec<_>>()}});
                    serde_json::to_writer(&mut *out, &v)?;
                    writeln!(out)?;
                } else {
                    for d in docs {
                        writeln!(
                            out,
                            "{}\t{}\t{}\t{}\t{}",
                            d.name,
                            match d.receipt.provider {
                                receipt::Provider::Mojang => "mojang",
                                receipt::Provider::Zulu => "zulu",
                            },
                            d.receipt.major_version,
                            d.receipt.version.as_deref().unwrap_or("unknown"),
                            d.receipt.executable
                        )?
                    }
                }
                Ok(0)
            }
            Some("remove") => {
                let parsed = args
                    .get(2)
                    .filter(|v| !v.is_empty())
                    .and_then(|name| parse_store(&args[3..], false).ok().map(|p| (name, p)));
                let Some((name, (store, _))) = parsed else {
                    writeln!(
                        err,
                        "usage: jmcl-core java runtime remove NAME [--store DIR]"
                    )?;
                    return Ok(2);
                };
                if let Err(e) = store::remove(&store, name) {
                    writeln!(err, "managed runtime remove failed: {e}")?;
                    return Ok(1);
                }
                writeln!(out, "removed\t{name}")?;
                Ok(0)
            }
            Some("install") => {
                writeln!(
                    err,
                    "Rust rewrite method is not implemented; do not use as a replacement Core"
                )?;
                Ok(1)
            }
            _ => {
                writeln!(
                    err,
                    "usage: jmcl-core java runtime <list|install|remove> [options]"
                )?;
                Ok(2)
            }
        },
        Some("select") => select::cli(&args[1..], out, err),
        _ => {
            writeln!(
                err,
                "usage: jmcl-core java <detect|select|runtime> [options]"
            )?;
            Ok(2)
        }
    }
}

fn object_params<T: serde::de::DeserializeOwned>(raw: &RawValue) -> Result<T, serde_json::Error> {
    if !raw.get().trim_start().starts_with('{') {
        return serde_json::from_str("!");
    }
    serde_json::from_str(raw.get())
}

pub mod managed;

pub mod select;

pub mod runtime_plan;

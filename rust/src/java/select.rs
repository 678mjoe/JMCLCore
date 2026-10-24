//! Version-specific Java selection. Explicit source selection never falls back.
use super::{detect, managed};
use crate::{
    metadata::{self, Source},
    protocol::RpcError,
    version::{self, Response, Service, Transport},
};
use serde::Deserialize;
use serde_json::{Value, value::RawValue};
use std::io::{self, Write};
#[derive(Deserialize)]
struct Params {
    id: String,
    #[serde(default)]
    source: Source,
    arch: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
}
pub fn required_major(metadata: &Value) -> Result<u16, &'static str> {
    match metadata.get("javaVersion").filter(|v| !v.is_null()) {
        None => Ok(8),
        Some(v) => v
            .get("majorVersion")
            .and_then(Value::as_u64)
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| *n > 0)
            .ok_or("InvalidJavaRequirement"),
    }
}
fn fetch_error(e: metadata::Error) -> RpcError {
    match e {
        metadata::Error::VersionNotFound => RpcError::new(
            "VERSION_NOT_FOUND",
            "The requested version is not in the selected manifest",
        ),
        _ => RpcError::new("METADATA_ERROR", "Failed to resolve the Java requirement"),
    }
}
fn parse(params: Option<&RawValue>) -> Result<Params, RpcError> {
    let raw = params.ok_or_else(|| super::invalid("java.select requires a version id"))?;
    let p: Params = super::object_params(raw)
        .map_err(|_| super::invalid("java.select parameters are invalid"))?;
    if p.arch
        .as_deref()
        .is_some_and(|a| !matches!(a, "x86" | "x86_64" | "arm" | "arm64"))
    {
        return Err(super::invalid("java.select parameters are invalid"));
    }
    if p.id.is_empty() {
        return Err(super::invalid("version id must not be empty"));
    }
    Ok(p)
}
pub fn handle_with<T: Transport>(
    params: Option<&RawValue>,
    service: &Service<T>,
    env: &detect::Environment,
) -> Result<Response, RpcError> {
    let p = parse(params)?;
    resolve(p, service, env)
}
fn resolve<T: Transport>(
    p: Params,
    service: &Service<T>,
    env: &detect::Environment,
) -> Result<Response, RpcError> {
    let metadata = service.get(&p.id, p.source).map_err(fetch_error)?;
    let major = required_major(&metadata).map_err(|_| {
        RpcError::new(
            "INVALID_METADATA",
            "Version metadata contains an invalid Java requirement",
        )
    })?;
    let host = managed::host_arch()
        .filter(|_| managed::host_os().is_some())
        .ok_or_else(|| {
            RpcError::new("UNSUPPORTED_PLATFORM", "The host platform is not supported")
        })?;
    let arch = p.arch.as_deref().unwrap_or(host);
    let d = detect::detect(env, &p.paths)
        .map_err(|_| RpcError::new("JAVA_NOT_FOUND", "No Java executable candidates were found"))?;
    let runtime = detect::select_compatible(&d, major, arch).map_err(|_| {
        RpcError::new(
            "JAVA_INCOMPATIBLE",
            "No Java runtime matches the required major version and architecture",
        )
    })?;
    Ok(Response {
        source: p.source,
        result: serde_json::json!({"version_id":p.id,"required_major":major,"required_arch":arch,"runtime":runtime,"probe_failures":d.failures}),
    })
}
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Response, RpcError>> {
    if method != "java.select" {
        return None;
    }
    Some((|| {
        let p = parse(params)?;
        let transport = version::HttpTransport::new().map_err(fetch_error)?;
        resolve(
            p,
            &Service::new(transport, ".jmclcore/metadata-cache"),
            &std::env::vars_os().collect(),
        )
    })())
}
const USAGE: &str =
    "usage: jmcl-core java select ID [--path PATH] [--source SOURCE] [--arch ARCH] [--json]";
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    let parse = || -> Option<(Params, bool)> {
        let id = args.first()?.clone();
        if id.is_empty() {
            return None;
        }
        let mut p = Params {
            id,
            source: Source::Official,
            arch: None,
            paths: vec![],
        };
        let mut json = false;
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--json" {
                json = true;
                i += 1;
                continue;
            }
            let value = args.get(i + 1)?;
            match args[i].as_str() {
                "--path" if !value.is_empty() => p.paths.push(value.clone()),
                "--source" => p.source = Source::parse(value)?,
                "--arch" => p.arch = Some(detect::architecture(value)?.into()),
                _ => return None,
            };
            i += 2;
        }
        Some((p, json))
    };
    let Some((p, json)) = parse() else {
        writeln!(err, "{USAGE}")?;
        return Ok(2);
    };
    let service = match version::HttpTransport::new() {
        Ok(t) => Service::new(t, ".jmclcore/metadata-cache"),
        Err(e) => {
            writeln!(err, "failed to fetch version metadata: {}", e.name())?;
            return Ok(1);
        }
    };
    let metadata = match service.get(&p.id, p.source) {
        Ok(v) => v,
        Err(e) => {
            writeln!(err, "failed to fetch version metadata: {}", e.name())?;
            return Ok(1);
        }
    };
    let major = match required_major(&metadata) {
        Ok(v) => v,
        Err(_) => {
            writeln!(err, "version metadata contains an invalid Java requirement")?;
            return Ok(1);
        }
    };
    let Some(host) = managed::host_arch().filter(|_| managed::host_os().is_some()) else {
        writeln!(err, "unsupported host platform: UnsupportedPlatform")?;
        return Ok(1);
    };
    let arch = p.arch.as_deref().unwrap_or(host);
    let d = match detect::detect(&std::env::vars_os().collect(), &p.paths) {
        Ok(d) => d,
        Err(e) => {
            writeln!(err, "Java detection failed: {e}")?;
            return Ok(1);
        }
    };
    let runtime = match detect::select_compatible(&d, major, arch) {
        Ok(v) => v,
        Err(_) => {
            writeln!(
                err,
                "no compatible Java found; required Java {major} for {arch}"
            )?;
            return Ok(1);
        }
    };
    if json {
        writeln!(
            out,
            "{}",
            serde_json::json!({"protocol":1,"source":p.source,"result":{"version_id":p.id,"required_major":major,"required_arch":arch,"runtime":runtime}})
        )?;
    } else {
        writeln!(
            out,
            "version\t{}\nrequiredJava\t{major}\nexecutable\t{}\njavaVersion\t{}\narchitecture\t{}\nvendor\t{}",
            p.id,
            runtime.executable,
            runtime
                .version
                .as_deref()
                .unwrap_or(&runtime.specification_version),
            runtime.os_arch,
            runtime.vendor.as_deref().unwrap_or("unknown")
        )?;
    }
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_zero() {
        assert_eq!(required_major(&serde_json::json!({})), Ok(8));
        assert!(required_major(&serde_json::json!({"javaVersion":{"majorVersion":0}})).is_err());
        assert_eq!(
            required_major(&serde_json::json!({"javaVersion":{"majorVersion":21}})),
            Ok(21)
        );
    }
}

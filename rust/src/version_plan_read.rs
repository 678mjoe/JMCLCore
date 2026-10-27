//! Conditional vanilla install.plan only. No installation or loader resolution.
use super::*;
use crate::auth::json::Json;

pub(super) struct Request {
    id: Vec<u8>,
    source: Source,
    os: Option<String>,
    arch: Option<String>,
}
pub(super) fn selected(raw: Option<&RawValue>) -> bool {
    let Some(raw) = raw else { return false };
    let Ok(value) = Json::parse(raw.get()) else {
        return false;
    };
    ["fabric_loader", "neoforge_version", "forge_version"]
        .iter()
        .all(|k| {
            matches!(value.field(k), Ok(None)) || matches!(value.field(k), Ok(Some(v)) if v.null())
        })
}
pub(super) fn selected_value(value: Option<&Value>) -> bool {
    value.and_then(Value::as_object).is_some_and(|p| {
        ["fabric_loader", "neoforge_version", "forge_version"]
            .iter()
            .all(|k| p.get(*k).is_none_or(Value::is_null))
    })
}
fn tag(value: Json<'_>, tags: &[&str]) -> Result<String, ()> {
    // Only the declared enum is materialized. Unknown fields retain raw tokens.
    let v: Value = serde_json::from_str(value.0).map_err(|_| ())?;
    enum_tag(&v, tags).map(|i| tags[i].to_owned()).ok_or(())
}
pub(super) fn prepare(raw: Option<&RawValue>) -> Result<Request, RpcError> {
    let raw = raw.ok_or_else(|| invalid("install.plan requires a version id"))?;
    let bad = || param_error("install.plan");
    let value = Json::parse(raw.get()).map_err(|_| bad())?;
    // Eligibility is a routing decision, never a new public vanilla-only error.
    // Callers invoke this only after selected; non-null loaders retain old routes.
    let source = match value.field("source").map_err(|_| bad())? {
        None => Source::Official,
        Some(v) => Source::parse(&tag(v, &["official", "bmclapi"]).map_err(|_| bad())?).unwrap(),
    };
    let optional = |key, tags: &[&str]| -> Result<Option<String>, RpcError> {
        match value.field(key).map_err(|_| bad())? {
            None => Ok(None),
            Some(v) if v.null() => Ok(None),
            Some(v) => tag(v, tags).map(Some).map_err(|_| bad()),
        }
    };
    let os = optional("os", &["windows", "linux", "osx"])?;
    let arch = optional("arch", &["x86", "x86_64", "arm", "arm64"])?;
    let id = value.required("id").map_err(|_| bad())?;
    let id = if id.0.starts_with('"') {
        id.string().map_err(|_| bad())?.as_bytes().to_vec()
    } else {
        id.array()
            .map_err(|_| bad())?
            .into_iter()
            .map(|v| {
                crate::json_number::dynamic_raw(v.0, 8)
                    .map(|n| n as u8)
                    .map_err(|_| bad())
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if id.is_empty() {
        return Err(invalid("version id must not be empty"));
    }
    Ok(Request {
        id,
        source,
        os,
        arch,
    })
}
pub(super) fn prepare_value(value: &Value) -> Result<Request, RpcError> {
    let raw = serde_json::value::to_raw_value(value).expect("existing JSON Value serializes");
    prepare(Some(&raw))
}
fn target(request: &Request) -> Result<Target, Error> {
    let mut target = Target::host()?;
    if let Some(os) = &request.os {
        target.os = os.clone();
    }
    if let Some(arch) = &request.arch {
        target.arch = arch.clone();
    }
    Ok(target)
}
fn attempt<T: Transport>(
    svc: &Service<T>,
    request: &Request,
    metadata_source: Source,
    target: Target,
) -> Result<Value, Error> {
    let metadata = svc.get_read(&request.id, metadata_source)?;
    let descriptor = &metadata.value()["assetIndex"];
    let assets = if descriptor.is_null() {
        None
    } else {
        Some(svc.assets(descriptor, metadata_source)?)
    };
    let plan = plan::build_read(&metadata, assets.as_ref(), request.source, target)?;
    Ok(serde_json::to_value(plan).expect("validated read plan serializes"))
}
fn planned<T: Transport>(
    svc: &Service<T>,
    request: &Request,
    target: Target,
) -> Result<Value, Error> {
    attempt(svc, request, request.source, target.clone()).or_else(|e| {
        if request.source == Source::Bmclapi {
            attempt(svc, request, Source::Official, target)
        } else {
            Err(e)
        }
    })
}
pub(super) fn rpc<T: Transport>(svc: &Service<T>, request: Request) -> Result<Response, RpcError> {
    let target = target(&request)
        .map_err(|_| RpcError::new("UNSUPPORTED_PLATFORM", "The host platform is not supported"))?;
    let result = planned(svc, &request, target).map_err(|e| map_error("install.plan", e))?;
    Ok(Response {
        source: request.source,
        result,
    })
}
pub(super) fn execute<T: Transport>(
    svc: &Service<T>,
    request: &Request,
    json: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<u8> {
    let target = match target(request) {
        Ok(t) => t,
        Err(e) => {
            writeln!(err, "unsupported host platform: {}", e.name())?;
            return Ok(1);
        }
    };
    let result = match planned(svc, request, target) {
        Ok(p) => p,
        Err(e) => {
            writeln!(
                err,
                "failed to create install plan from {}: {}",
                request.source.display(),
                e.name()
            )?;
            return Ok(1);
        }
    };
    if json {
        #[derive(Serialize)]
        struct Document {
            protocol: u32,
            source: Source,
            result: Value,
        }
        serde_json::to_writer(
            &mut *out,
            &Document {
                protocol: 1,
                source: request.source,
                result,
            },
        )?;
        out.write_all(b"\n")?;
        return Ok(0);
    }
    if request.source == Source::Bmclapi {
        out.write_all(b"# source\tBMCLAPI\n")?;
    }
    out.write_all(b"version\t")?;
    out.write_all(&request.id)?;
    out.write_all(b"\n")?;
    writeln!(
        out,
        "target\t{}\t{}",
        result["target"]["os"].as_str().unwrap(),
        result["target"]["arch"].as_str().unwrap()
    )?;
    for (label, key) in [
        ("files", "files"),
        ("classpath", "classpath"),
        ("nativeExtractions", "native_extractions"),
        ("assetProjections", "asset_projections"),
    ] {
        writeln!(out, "{label}\t{}", result[key].as_array().unwrap().len())?;
    }
    writeln!(out, "downloadBytes\t{}", result["total_download_size"])?;
    Ok(0)
}
#[cfg(all(test, unix))]
#[path = "version_plan_read_tests.rs"]
mod tests;

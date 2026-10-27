//! Standalone version metadata reads only; never resolve or plan.
use super::*;

pub(super) struct Request {
    pub id: Vec<u8>,
    pub source: Source,
}
pub(super) fn prepare(value: Option<&Value>) -> Result<Request, RpcError> {
    let value = value.ok_or_else(|| invalid("version.get requires an id"))?;
    let fields = value
        .as_object()
        .ok_or_else(|| param_error("version.get"))?;
    let source = match fields.get("source") {
        None => Source::Official,
        Some(v) => match enum_tag(v, &["official", "bmclapi"]) {
            Some(0) => Source::Official,
            Some(1) => Source::Bmclapi,
            _ => return Err(param_error("version.get")),
        },
    };
    let value = fields.get("id").ok_or_else(|| param_error("version.get"))?;
    let id = if let Some(s) = value.as_str() {
        s.as_bytes().to_vec()
    } else {
        value
            .as_array()
            .ok_or_else(|| param_error("version.get"))?
            .iter()
            .map(|v| {
                crate::json_number::dynamic_raw(&v.to_string(), 8)
                    .map(|n| n as u8)
                    .map_err(|_| param_error("version.get"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if id.is_empty() {
        return Err(invalid("version id must not be empty"));
    }
    Ok(Request { id, source })
}

impl<T: Transport> Service<T> {
    fn get_read(&self, id: &[u8], source: Source) -> Result<metadata::ReadMetadata, Error> {
        let (bytes, selected_id) = self.fetch_metadata_bytes(id, source)?;
        let metadata = metadata::ReadMetadata::parse(&bytes)?;
        if metadata.id() != selected_id {
            return Err(Error::MetadataIdMismatch);
        }
        Ok(metadata)
    }
}
pub(super) fn rpc<T: Transport>(
    service: &Service<T>,
    request: Request,
) -> Result<Response, RpcError> {
    let result = service
        .get_read(&request.id, request.source)
        .map_err(|e| map_error("version.get", e))?;
    Ok(Response {
        source: request.source,
        result: result.into_value(),
    })
}

pub(super) fn execute<T: Transport>(
    service: &Service<T>,
    request: &Request,
    json: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<u8> {
    let metadata = match service.get_read(&request.id, request.source) {
        Ok(m) => m,
        Err(Error::VersionNotFound) => {
            err.write_all(b"Minecraft version not found: ")?;
            err.write_all(&request.id)?;
            err.write_all(b"\n")?;
            return Ok(1);
        }
        Err(e) => {
            writeln!(
                err,
                "failed to fetch Minecraft version metadata from {}: {}",
                request.source.display(),
                e.name()
            )?;
            return Ok(1);
        }
    };
    let value = metadata.value();
    if json {
        #[derive(Serialize)]
        struct Document<'a> {
            protocol: u32,
            source: Source,
            result: &'a Value,
        }
        serde_json::to_writer(
            &mut *out,
            &Document {
                protocol: 1,
                source: request.source,
                result: value,
            },
        )?;
        out.write_all(b"\n")?;
        return Ok(0);
    }
    if request.source == Source::Bmclapi {
        out.write_all(b"# source\tBMCLAPI\n")?;
    }
    out.write_all(b"id\t")?;
    out.write_all(metadata.id())?;
    out.write_all(b"\n")?;
    for k in ["type", "mainClass"] {
        writeln!(out, "{k}\t{}", value[k].as_str().unwrap())?;
    }
    writeln!(
        out,
        "libraries\t{}",
        value["libraries"].as_array().unwrap().len()
    )?;
    writeln!(
        out,
        "client\t{}",
        value["downloads"]["client"]["url"].as_str().unwrap()
    )?;
    if !value["assetIndex"].is_null() {
        writeln!(
            out,
            "assetIndex\t{}",
            value["assetIndex"]["id"].as_str().unwrap()
        )?;
    }
    if !value["javaVersion"].is_null() {
        writeln!(
            out,
            "java\t{}\t{}",
            value["javaVersion"]["majorVersion"],
            value["javaVersion"]["component"].as_str().unwrap()
        )?;
    }
    writeln!(
        out,
        "arguments\t{}",
        if value["arguments"].is_null() {
            "legacy"
        } else {
            "modern"
        }
    )?;
    Ok(0)
}

#[cfg(unix)]
fn native_request(args: &[std::ffi::OsString]) -> Option<Result<(Request, bool), ()>> {
    use std::os::unix::ffi::OsStrExt;
    if args.first()?.to_str() != Some("version")
        || args.get(1)?.to_str() != Some("show")
        || args.iter().all(|s| s.to_str().is_some())
    {
        return None;
    }
    Some((|| {
        let id = args.get(2).ok_or(())?.as_bytes().to_vec();
        if id.is_empty() {
            return Err(());
        }
        let mut source = Source::Official;
        let mut json = false;
        let mut i = 3;
        while i < args.len() {
            match args[i].to_str() {
                Some("--json") => {
                    json = true;
                    i += 1;
                }
                Some("--source") => {
                    source = Source::parse(args.get(i + 1).and_then(|s| s.to_str()).ok_or(())?)
                        .ok_or(())?;
                    i += 2;
                }
                _ => return Err(()),
            }
        }
        Ok((Request { id, source }, json))
    })())
}
/// Native-byte argv adapter claims only version show. UTF-8 argv retains its CLI.
pub fn cli_os(
    args: &[std::ffi::OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<std::io::Result<u8>> {
    #[cfg(unix)]
    {
        let (request, json) = match native_request(args)? {
            Ok(v) => v,
            Err(()) => return Some(writeln!(err, "{}", cli_usage("version.get")).map(|_| 2)),
        };
        let transport = match HttpTransport::new() {
            Ok(t) => t,
            Err(e) => {
                return Some(
                    writeln!(
                        err,
                        "failed to fetch Minecraft version metadata from {}: {}",
                        request.source.display(),
                        e.name()
                    )
                    .map(|_| 1),
                );
            }
        };
        Some(execute(
            &Service::new(transport, ".jmclcore/metadata-cache"),
            &request,
            json,
            out,
            err,
        ))
    }
    #[cfg(not(unix))]
    {
        let _ = (args, out, err);
        None
    }
}

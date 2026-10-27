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

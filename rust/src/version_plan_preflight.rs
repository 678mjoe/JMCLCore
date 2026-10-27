//! Pure install.plan preflight; host lookup is injected, never a resolver.
use super::*;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Params(RpcError),
    Host(Error),
    Conflict,
}
pub(super) fn target(
    os: Option<&str>,
    arch: Option<&str>,
    loaders: usize,
    host: impl FnOnce() -> Result<Target, Error>,
) -> Result<Target, Failure> {
    if loaders > 1 {
        return Err(Failure::Conflict);
    }
    let mut target = host().map_err(Failure::Host)?;
    if let Some(os) = os {
        target.os = os.into();
    }
    if let Some(arch) = arch {
        target.arch = arch.into();
    }
    Ok(target)
}
pub(super) fn prepare(
    value: &Value,
    host: impl FnOnce() -> Result<Target, Error>,
) -> Result<Target, Failure> {
    let canonical = canonical_params_view("install.plan", Some(value)).map_err(Failure::Params)?;
    let value = canonical.as_deref();
    validate_params("install.plan", value).map_err(Failure::Params)?;
    let p = value
        .and_then(Value::as_object)
        .expect("validated planning object");
    target(
        p.get("os").and_then(Value::as_str),
        p.get("arch").and_then(Value::as_str),
        loader_count(p).expect("validated loader fields"),
        host,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;
    #[test]
    fn injected_host_failure_exposes_conflict_order() {
        let calls = Cell::new(0);
        let value = json!({"id":"demo","fabric_loader":"a","forge_version":"b","os":"linux","arch":"x86_64"});
        let result = prepare(&value, || {
            calls.set(calls.get() + 1);
            Err(Error::Plan("UnsupportedOperatingSystem"))
        });
        assert_eq!(result, Err(Failure::Conflict));
        assert_eq!(calls.get(), 0);
    }
}

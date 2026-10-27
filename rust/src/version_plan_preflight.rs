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
    let mut target = host().map_err(Failure::Host)?;
    if let Some(os) = os {
        target.os = os.into();
    }
    if let Some(arch) = arch {
        target.arch = arch.into();
    }
    if loaders > 1 {
        return Err(Failure::Conflict);
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
    fn host_failure_precedes_conflict_even_with_complete_overrides() {
        let calls = Cell::new(0);
        let value = json!({"id":"demo","fabric_loader":"a","forge_version":"b","os":"linux","arch":"x86_64"});
        let result = prepare(&value, || {
            calls.set(calls.get() + 1);
            Err(Error::Plan("UnsupportedOperatingSystem"))
        });
        assert_eq!(
            result,
            Err(Failure::Host(Error::Plan("UnsupportedOperatingSystem")))
        );
        assert_eq!(calls.get(), 1);
    }

    fn host() -> Target {
        Target {
            os: "linux".into(),
            arch: "x86_64".into(),
            os_version: Some("kept".into()),
        }
    }
    #[test]
    fn schema_and_empty_id_precede_host_and_conflict() {
        for (value, expected) in [
            (
                json!({"id":"","os":"bad","fabric_loader":"a","forge_version":"b"}),
                param_error("install.plan"),
            ),
            (
                json!({"id":"","fabric_loader":false}),
                param_error("install.plan"),
            ),
            (json!({"id":[],"source":null}), param_error("install.plan")),
            (
                json!({"id":"","fabric_loader":"a","forge_version":"b"}),
                invalid("version id must not be empty"),
            ),
            (json!({"id":[]}), invalid("version id must not be empty")),
        ] {
            let calls = Cell::new(0);
            let result = prepare(&value, || {
                calls.set(calls.get() + 1);
                Err(Error::Plan("UnsupportedArchitecture"))
            });
            assert_eq!(result, Err(Failure::Params(expected)));
            assert_eq!(calls.get(), 0);
        }
    }
    #[test]
    fn valid_host_overrides_then_conflict_or_success_are_exact() {
        for loaders in [0, 1, 2] {
            let calls = Cell::new(0);
            let mut value = json!({"id":"demo","os":"0_2","arch":3});
            if loaders >= 1 {
                value["fabric_loader"] = "".into();
            } // Presence, not nonemptiness.
            if loaders >= 2 {
                value["forge_version"] = "b".into();
            }
            let result = prepare(&value, || {
                calls.set(calls.get() + 1);
                Ok(host())
            });
            assert_eq!(calls.get(), 1);
            if loaders == 2 {
                assert_eq!(result, Err(Failure::Conflict));
            } else {
                assert_eq!(
                    result,
                    Ok(Target {
                        os: "osx".into(),
                        arch: "arm64".into(),
                        os_version: Some("kept".into())
                    })
                );
            }
        }
        assert_eq!(
            prepare(&json!({"id":"demo","os":null,"arch":null}), || Ok(host())),
            Ok(host())
        );
    }
    #[test]
    fn host_failure_never_becomes_success_from_overrides() {
        for loaders in [0, 1, 2] {
            let calls = Cell::new(0);
            assert_eq!(
                target(Some("windows"), Some("arm64"), loaders, || {
                    calls.set(calls.get() + 1);
                    Err(Error::Plan("UnsupportedArchitecture"))
                }),
                Err(Failure::Host(Error::Plan("UnsupportedArchitecture")))
            );
            assert_eq!(calls.get(), 1);
        }
    }
}

//! Vanilla preparation keeps the original prepare-only schema and byte identity.
use super::*;
#[derive(Debug)]
pub(crate) struct Request {
    pub id: Vec<u8>,
    pub directory: Vec<u8>,
    pub source: Source,
    pub os: Option<String>,
    pub arch: Option<String>,
}
pub(crate) fn parse(raw: Option<&RawValue>) -> Result<Request, RpcError> {
    let bad = || RpcError::new("INVALID_PARAMS", "install.prepare parameters are invalid");
    let raw =
        raw.ok_or_else(|| RpcError::new("INVALID_PARAMS", "install.prepare requires an id"))?;
    let p = Json::parse(raw.get()).map_err(|_| bad())?;
    let p = (|| -> Result<_, ()> {
        let id = bytes(p.required("id")?)?;
        let directory = p
            .field("directory")?
            .map(bytes)
            .unwrap_or_else(|| Ok(b".minecraft".to_vec()))?;
        for key in ["fabric_loader", "neoforge_version", "forge_version"] {
            if let Some(v) = p.field(key)?
                && !v.null()
            {
                return Err(());
            }
        }
        let source = match p.field("source")? {
            None => Source::Official,
            Some(v) => Source::parse(&tag(v, &["official", "bmclapi"])?).ok_or(())?,
        };
        let optional = |key, tags: &[&str]| match p.field(key)? {
            None => Ok(None),
            Some(v) if v.null() => Ok(None),
            Some(v) => tag(v, tags).map(Some),
        };
        Ok(Request {
            id,
            directory,
            source,
            os: optional("os", &["windows", "linux", "osx"])?,
            arch: optional("arch", &["x86", "x86_64", "arm", "arm64"])?,
        })
    })()
    .map_err(|_| bad())?;
    if p.id.is_empty() || p.directory.is_empty() {
        return Err(RpcError::new(
            "INVALID_PARAMS",
            "install.prepare id and directory must not be empty",
        ));
    }
    Ok(p)
}
pub(crate) fn run<M: version::Transport>(
    out: &mut impl Write,
    id: &str,
    p: Request,
    svc: &version::Service<M>,
) -> io::Result<()> {
    let mut target = match Target::host() {
        Ok(t) => t,
        Err(_) => {
            return protocol::error(
                out,
                Some(id),
                "UNSUPPORTED_PLATFORM",
                "The host platform is not supported",
            );
        }
    };
    if let Some(os) = p.os {
        target.os = os;
    }
    if let Some(arch) = p.arch {
        target.arch = arch;
    }
    let plan = match svc.vanilla_read_plan(&p.id, p.source, target) {
        Ok(p) => p,
        Err(e) => return error(out, id, super::super::rpc::planning_error(e)),
    };
    let root = match crate::store::path_from_bytes(&p.directory)
        .and_then(crate::store::ArtifactRoot::open)
    {
        Ok(root) => root,
        Err(_) => {
            return protocol::error(
                out,
                Some(id),
                "INSTALL_ROOT_ERROR",
                "Failed to open the install root",
            );
        }
    };
    let prepared = match plan.prepare_game_native(&root) {
        Ok(p) => p,
        Err(e) => return error(out, id, super::super::rpc::prepare_error(e)),
    };
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        protocol: u8,
        id: &'a str,
        event: &'static str,
        source: Source,
        result: ResultPayload,
    }
    #[derive(serde::Serialize)]
    struct ResultPayload {
        stage: &'static str,
        version_id: BytePath,
        #[serde(flatten)]
        prepared: Preparation,
    }
    serde_json::to_writer(
        &mut *out,
        &Envelope {
            protocol: 1,
            id,
            event: "result",
            source: p.source,
            result: ResultPayload {
                stage: "prepare",
                version_id: BytePath(p.id),
                prepared,
            },
        },
    )?;
    out.write_all(b"\n")
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw(s: &str) -> Box<RawValue> {
        RawValue::from_string(s.into()).unwrap()
    }
    #[test]
    fn prepare_schema_keeps_bytes_and_ignores_execute_only_fields() {
        let p = parse(Some(&raw(r#"{"id":[255],"directory":[254],"workers":null,"retries":false,"java_policy":null,"java_paths":false,"store_directory":null}"#))).unwrap();
        assert_eq!(p.id, [255]);
        assert_eq!(p.directory, [254]);
        assert_eq!(
            parse(Some(&raw(r#"{"id":"v"}"#))).unwrap().directory,
            b".minecraft"
        );
    }
    #[test]
    fn prepare_schema_preserves_validation_order() {
        for s in [
            "null",
            "{}",
            r#"{"id":"","source":null}"#,
            r#"{"id":"","directory":null}"#,
            r#"{"id":"v","os":"bogus"}"#,
            r#"{"id":[256]}"#,
        ] {
            assert_eq!(
                parse(Some(&raw(s))).unwrap_err().message,
                "install.prepare parameters are invalid"
            );
        }
        for s in [r#"{"id":""}"#, r#"{"id":"v","directory":[]}"#] {
            assert_eq!(
                parse(Some(&raw(s))).unwrap_err().message,
                "install.prepare id and directory must not be empty"
            );
        }
        assert_eq!(
            parse(None).unwrap_err().message,
            "install.prepare requires an id"
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod execution_tests {
    use super::*;
    use serde_json::json;
    use std::{cell::RefCell, collections::VecDeque, os::unix::ffi::OsStrExt};
    struct Fake(RefCell<VecDeque<(String, Vec<u8>)>>);
    impl version::Transport for Fake {
        fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, crate::metadata::Error> {
            let (expected, body) = self
                .0
                .borrow_mut()
                .pop_front()
                .expect("unexpected network operation");
            assert_eq!(url, expected);
            Ok(body)
        }
    }
    fn service(owner: &std::path::Path) -> version::Service<Fake> {
        fixture(owner, false)
    }
    fn fixture(owner: &std::path::Path, native: bool) -> version::Service<Fake> {
        let mut metadata = json!({"id":[255],"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"a".repeat(40),"size":1,"url":"https://example.invalid/never-fetch-client"}}});
        if native {
            metadata["libraries"] = json!([{"name":"fixture:native:1", "natives":{"linux":"natives-linux"}, "downloads":{"classifiers":{"natives-linux":{"path":"fixture/native.jar","sha1":"b".repeat(40),"size":1,"url":"https://example.invalid/never-fetch-native"}}}}]);
        }
        let manifest = json!({"latest":{"release":[255],"snapshot":"s"},"versions":[{"id":[255],"type":"release","url":"https://example.invalid/metadata","time":"t","releaseTime":"r"}]});
        version::Service::new(
            Fake(RefCell::new(VecDeque::from([
                (
                    Source::Official.manifest_url().into(),
                    serde_json::to_vec(&manifest).unwrap(),
                ),
                (
                    "https://example.invalid/metadata".into(),
                    serde_json::to_vec(&metadata).unwrap(),
                ),
            ]))),
            owner.join("metadata-cache"),
        )
    }
    #[test]
    fn missing_native_file_maps_cache_invalid_without_download() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("missing-native");
        let mut out = vec![];
        run(
            &mut out,
            "test",
            request(&root),
            &fixture(owner.path(), true),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            value["error"]["code"],
            "CACHE_INVALID",
            "{}",
            String::from_utf8_lossy(&out)
        );
        assert!(!root.join("fixture/native.jar").exists());
    }
    fn request(path: &std::path::Path) -> Request {
        Request {
            id: vec![255],
            directory: path.as_os_str().as_bytes().to_vec(),
            source: Source::Official,
            os: Some("linux".into()),
            arch: Some("x86_64".into()),
        }
    }
    #[test]
    fn zero_native_preparation_preserves_byte_identity_and_has_no_install_receipt() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join(std::ffi::OsStr::from_bytes(b"game-\xff"));
        let mut out = vec![];
        run(&mut out, "test", request(&root), &service(owner.path())).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            result["event"],
            "result",
            "{}",
            String::from_utf8_lossy(&out)
        );
        assert_eq!(result["result"]["stage"], "prepare");
        assert_eq!(result["result"]["version_id"], json!([255]));
        assert_eq!(result["result"]["native_files"], 0);
        assert_eq!(result["result"]["projections_created"], 0);
        assert_eq!(result["result"]["projections_cached"], 0);
        assert_eq!(out.iter().filter(|b| **b == b'\n').count(), 1);
        let names: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names.len(),
            1,
            "only native preparation directory expected: {names:?}"
        );
    }
    #[test]
    fn blocked_root_does_not_write_receipts_or_modify_existing_file() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("file");
        std::fs::write(&root, b"canary").unwrap();
        let mut out = vec![];
        run(&mut out, "test", request(&root), &service(owner.path())).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(result["error"]["code"], "INSTALL_ROOT_ERROR");
        assert_eq!(std::fs::read(&root).unwrap(), b"canary");
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    fn public_route_rejects_invalid_bytes_before_any_network_or_root_access() {
        for body in [
            r#"{"id":[],"directory":[255]}"#,
            r#"{"id":[256]}"#,
            r#"{"id":"v","directory":null}"#,
        ] {
            let raw = RawValue::from_string(body.into()).unwrap();
            let mut out = vec![];
            super::super::super::rpc::handle(&mut out, "request", "install.prepare", Some(&raw))
                .unwrap()
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(value["error"]["code"], "INVALID_PARAMS");
        }
    }
    #[test]
    fn ordinary_prepare_schema_matches_original_rust_route() {
        for body in [
            r#"{"id":"v"}"#,
            r#"{"id":"v","directory":"owned","source":"bmclapi","os":"linux","arch":"arm64","forge_version":null}"#,
        ] {
            let raw = RawValue::from_string(body.into()).unwrap();
            let new = parse(Some(&raw)).unwrap();
            let super::super::super::rpc::Request::Prepare(old) =
                super::super::super::rpc::parse("install.prepare", Some(&raw)).unwrap()
            else {
                panic!()
            };
            assert_eq!(new.id, old.selection.id.as_bytes());
            assert_eq!(
                crate::store::path_from_bytes(&new.directory).unwrap(),
                old.directory.path().unwrap()
            );
            assert_eq!(new.source, old.selection.source);
            assert_eq!(new.os, old.selection.os);
            assert_eq!(new.arch, old.selection.arch);
        }
    }
}

//! Explicit validate:false creation only. No resolver or operational read model.
use super::{LookupId, RpcError, invalid, validate_id};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Debug)]
struct Bytes(Vec<u8>);
impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        LookupId::deserialize(d).map(|v| Self(v.0))
    }
}
impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => s.serialize_str(text),
            Err(_) => self.0.serialize(s),
        }
    }
}
fn official() -> Bytes {
    Bytes(b"official".to_vec())
}
#[derive(Deserialize)]
struct Input {
    directory: Bytes,
    id: Bytes,
    version_id: Bytes,
    #[serde(default)]
    name: Option<Bytes>,
    #[serde(default)]
    fabric_loader: Option<Bytes>,
    #[serde(default)]
    neoforge_version: Option<Bytes>,
    #[serde(default)]
    forge_version: Option<Bytes>,
    #[serde(default = "official")]
    source: Bytes,
    validate: bool,
}

// Selection is deliberately narrow: omitted, null, malformed and true values
// continue through the existing String-based decoder and resolver path.
pub(super) fn selected(raw: Option<&RawValue>) -> bool {
    #[derive(Deserialize)]
    struct Gate<'a> {
        #[serde(borrow)]
        validate: &'a RawValue,
    }
    raw.filter(|raw| raw.get().trim_start().starts_with('{'))
        .and_then(|raw| serde_json::from_str::<Gate<'_>>(raw.get()).ok())
        .is_some_and(|gate| gate.validate.get() == "false")
}

pub(super) struct Prepared {
    directory: Bytes,
    manifest: ByteManifest,
}
#[derive(Serialize)]
struct ByteManifest {
    schema_version: u32,
    id: String,
    name: Bytes,
    version_id: Bytes,
    fabric_loader: Option<Bytes>,
    neoforge_version: Option<Bytes>,
    forge_version: Option<Bytes>,
    source: String,
}

pub(super) fn prepare(raw: Option<&RawValue>) -> Result<Prepared, RpcError> {
    if !selected(raw) {
        return Err(invalid("instance creation parameters are invalid"));
    }
    let input: Input = super::parse(
        raw,
        "instance creation requires directory, id, and version_id",
        "instance creation parameters are invalid",
    )?;
    if input.validate {
        return Err(invalid("instance creation parameters are invalid"));
    }
    // Preserve protocol/instance.zig checks before root opening and late manifest
    // validation. No UTF-8 requirement applies to display/version/loader bytes.
    if input.directory.0.is_empty() || input.id.0.is_empty() || input.version_id.0.is_empty() {
        return Err(invalid("instance creation parameters are out of range"));
    }
    let id = std::str::from_utf8(&input.id.0)
        .ok()
        .filter(|id| validate_id(id).is_ok())
        .ok_or_else(|| invalid("the instance id is invalid"))?
        .to_owned();
    let source = match input.source.0.as_slice() {
        b"official" => "official",
        b"bmclapi" => "bmclapi",
        _ => return Err(invalid("source must be official or bmclapi")),
    };
    if [
        &input.fabric_loader,
        &input.neoforge_version,
        &input.forge_version,
    ]
    .iter()
    .filter(|v| v.is_some())
    .count()
        > 1
    {
        return Err(invalid("only one loader field may be set"));
    }
    Ok(Prepared {
        directory: input.directory,
        manifest: ByteManifest {
            schema_version: 1,
            name: input.name.unwrap_or_else(|| Bytes(id.as_bytes().to_vec())),
            id,
            version_id: input.version_id,
            fabric_loader: input.fabric_loader,
            neoforge_version: input.neoforge_version,
            forge_version: input.forge_version,
            source: source.into(),
        },
    })
}
impl ByteManifest {
    fn validate(&self) -> Result<(), &'static str> {
        validate_id(&self.id)?;
        for (bytes, error) in [
            (Some(&self.name), "InvalidInstanceName"),
            (Some(&self.version_id), "InvalidVersionId"),
            (self.fabric_loader.as_ref(), "InvalidFabricLoaderVersion"),
            (self.neoforge_version.as_ref(), "InvalidNeoForgeVersion"),
            (self.forge_version.as_ref(), "InvalidForgeVersion"),
        ] {
            if let Some(bytes) = bytes
                && (bytes.0.is_empty() || bytes.0.len() > 256)
            {
                return Err(error);
            }
        }
        Ok(())
    }
    fn encode(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self).map_err(|_| "InvalidInstanceManifest")?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

impl Prepared {
    pub(super) fn execute(self) -> Result<serde_json::Value, RpcError> {
        use super::{Root, fail};
        let root_error = |_| fail("INSTANCES_ROOT_ERROR", "Failed to open the instances root");
        let path = crate::store::path_from_bytes(&self.directory.0).map_err(root_error)?;
        let root = Root::open_native(&path).map_err(root_error)?;
        // The pinned handler opens the root before create validates the manifest.
        self.manifest
            .validate()
            .map_err(|_| fail("INSTANCE_CREATE_FAILED", "Failed to create the instance"))?;
        root.create_with(&self.manifest.id, || {
            self.manifest
                .encode()
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                fail("INSTANCE_EXISTS", "An instance with this id already exists")
            } else {
                fail("INSTANCE_CREATE_FAILED", "Failed to create the instance")
            }
        })?;
        let m = &self.manifest;
        Ok(
            serde_json::json!({"stage":"instance.create", "id":m.id, "name":m.name,
            "version_id":m.version_id, "fabric_loader":m.fabric_loader,
            "neoforge_version":m.neoforge_version, "forge_version":m.forge_version,
            "source":m.source, "installed":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn raw(value: &Value) -> Box<RawValue> {
        RawValue::from_string(value.to_string()).unwrap()
    }
    fn input() -> Value {
        json!({"directory":"unused", "id":"demo", "version_id":"v", "validate":false})
    }
    #[test]
    fn selection_is_exclusive_to_literal_false() {
        for token in [
            "{}",
            r#"{"validate":true}"#,
            r#"{"validate":null}"#,
            r#"{"validate":0}"#,
            r#"{"validate":"false"}"#,
            r#"{"validate":false,"validate":true}"#,
            "[]",
        ] {
            let raw = RawValue::from_string(token.into()).unwrap();
            assert!(!selected(Some(&raw)), "{token}");
            assert!(prepare(Some(&raw)).is_err());
        }
        assert!(!selected(None));
        for validation in [None, Some(true)] {
            let mut value = input();
            if let Some(v) = validation {
                value["validate"] = json!(v);
            } else {
                value.as_object_mut().unwrap().remove("validate");
            }
            let raw = raw(&value);
            assert!(!selected(Some(&raw)));
            assert!(super::super::parse_create(Some(&raw)).unwrap().validate);
        }
        assert!(selected(Some(&raw(&input()))));
    }
    #[test]
    fn opaque_preflight_preserves_source_check_order() {
        let mut value = input();
        value["name"] = json!([255]);
        value["version_id"] = json!([254]);
        assert!(prepare(Some(&raw(&value))).is_ok());
        value["id"] = json!([255]);
        value["source"] = json!([255]);
        value["fabric_loader"] = json!([255]);
        value["forge_version"] = json!([255]);
        value["directory"] = json!([]);
        assert_eq!(
            prepare(Some(&raw(&value))).err().unwrap(),
            invalid("instance creation parameters are out of range")
        );
        value["directory"] = json!([255]);
        assert_eq!(
            prepare(Some(&raw(&value))).err().unwrap(),
            invalid("the instance id is invalid")
        );
        value["id"] = json!("demo");
        assert_eq!(
            prepare(Some(&raw(&value))).err().unwrap(),
            invalid("source must be official or bmclapi")
        );
        value["source"] = json!("official");
        assert_eq!(
            prepare(Some(&raw(&value))).err().unwrap(),
            invalid("only one loader field may be set")
        );
        value.as_object_mut().unwrap().remove("forge_version");
        assert!(prepare(Some(&raw(&value))).is_ok());
        value["id"] = json!([256]);
        value["directory"] = json!([]);
        assert_eq!(
            prepare(Some(&raw(&value))).err().unwrap(),
            invalid("instance creation parameters are invalid")
        );
    }
    #[test]
    fn byte_manifest_encoding_and_late_length_checks() {
        let mut value = input();
        value["name"] = json!([255]);
        value["version_id"] = json!([254]);
        value["fabric_loader"] = json!([253]);
        let p = prepare(Some(&raw(&value))).unwrap();
        assert_eq!(p.manifest.encode().unwrap(), b"{\"schema_version\":1,\"id\":\"demo\",\"name\":[255],\"version_id\":[254],\"fabric_loader\":[253],\"neoforge_version\":null,\"forge_version\":null,\"source\":\"official\"}\n");
        for (field, error) in [
            ("name", "InvalidInstanceName"),
            ("version_id", "InvalidVersionId"),
            ("fabric_loader", "InvalidFabricLoaderVersion"),
            ("neoforge_version", "InvalidNeoForgeVersion"),
            ("forge_version", "InvalidForgeVersion"),
        ] {
            let mut value = input();
            value[field] = json!(vec![255u8; 257]);
            let p = prepare(Some(&raw(&value))).unwrap();
            assert_eq!(p.manifest.validate(), Err(error));
        }
        let mut value = input();
        value["version_id"] = json!([65.00000000000000001_f64]);
        assert_eq!(
            prepare(Some(&raw(&value))).unwrap().manifest.version_id.0,
            b"A"
        );
        value["version_id"] = json!(["65.00000000000000001"]);
        assert!(prepare(Some(&raw(&value))).is_err());
    }

    struct Fixture {
        owner: tempfile::TempDir,
        root: std::path::PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let owner = tempfile::tempdir().unwrap();
            let root = owner.path().join("instances");
            std::fs::write(owner.path().join("sentinel"), b"untouched").unwrap();
            std::fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
            Self { owner, root }
        }
        fn request(&self, value: Value) -> Result<Value, RpcError> {
            // Reject every unguarded request before even selecting a handler.
            if value.get("validate") != Some(&Value::Bool(false)) {
                return Err(RpcError::new(
                    "FIXTURE_GUARD",
                    "literal validate:false required",
                ));
            }
            let directory: Bytes =
                serde_json::from_str(&value.get("directory").unwrap_or(&Value::Null).to_string())
                    .map_err(|_| RpcError::new("FIXTURE_GUARD", "explicit owned root required"))?;
            let path = crate::store::path_from_bytes(&directory.0)
                .map_err(|_| RpcError::new("FIXTURE_GUARD", "explicit owned root required"))?;
            if directory.0.is_empty()
                || path != self.root.as_os_str()
                || !self.root.starts_with(self.owner.path())
            {
                return Err(RpcError::new(
                    "FIXTURE_GUARD",
                    "explicit owned root required",
                ));
            }
            let raw = raw(&value);
            super::super::handle("instance.create", Some(&raw)).unwrap()
        }
        fn input(&self) -> Value {
            let mut v = input();
            v["directory"] = json!(self.root.to_str().unwrap());
            v
        }
        fn sentinels(&self) {
            assert_eq!(
                std::fs::read(self.owner.path().join("sentinel")).unwrap(),
                b"untouched"
            );
            assert_eq!(
                std::fs::read(self.owner.path().join("peer")).unwrap(),
                b"untouched"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let a = std::fs::metadata(self.owner.path().join("sentinel")).unwrap();
                let b = std::fs::metadata(self.owner.path().join("peer")).unwrap();
                assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
                assert_eq!(a.nlink(), 2);
            }
        }
    }
    #[test]
    fn guarded_false_request_writes_bytes_and_preserves_peers() {
        let f = Fixture::new();
        for field in [
            "name",
            "version_id",
            "fabric_loader",
            "neoforge_version",
            "forge_version",
        ] {
            let mut value = f.input();
            value["id"] = json!(field);
            value[field] = json!([255]);
            let expected = prepare(Some(&raw(&value)))
                .unwrap()
                .manifest
                .encode()
                .unwrap();
            let result = f.request(value.clone()).unwrap();
            assert_eq!(result[field], json!([255]));
            assert_eq!(result["installed"], false);
            let manifest = f.root.join(field).join("instance.json");
            assert_eq!(std::fs::read(&manifest).unwrap(), expected);
            assert!(f.root.join(field).join(".minecraft").is_dir());
            assert_eq!(f.request(value).unwrap_err().code, "INSTANCE_EXISTS");
            assert_eq!(std::fs::read(&manifest).unwrap(), expected);
            f.sentinels();
        }
        // Reject omitted/default/true/null validation and nonowned roots before dispatch.
        for token in [
            None,
            Some(json!(true)),
            Some(Value::Null),
            Some(json!("false")),
        ] {
            let mut value = f.input();
            if let Some(token) = token {
                value["validate"] = token;
            } else {
                value.as_object_mut().unwrap().remove("validate");
            }
            assert_eq!(f.request(value).unwrap_err().code, "FIXTURE_GUARD");
        }
        for directory in [Value::Null, json!(""), json!("elsewhere")] {
            let mut value = f.input();
            value["directory"] = directory;
            assert_eq!(f.request(value).unwrap_err().code, "FIXTURE_GUARD");
        }
        assert!(!f.root.join("demo").exists());
        f.sentinels();
    }
    #[test]
    fn guarded_false_errors_preserve_root_order_and_sentinels() {
        let f = Fixture::new();
        let mut value = f.input();
        value["name"] = json!([]);
        std::fs::write(&f.root, b"root-file").unwrap();
        assert_eq!(
            f.request(value.clone()).unwrap_err().code,
            "INSTANCES_ROOT_ERROR"
        );
        assert_eq!(std::fs::read(&f.root).unwrap(), b"root-file");
        std::fs::remove_file(&f.root).unwrap();
        assert_eq!(f.request(value).unwrap_err().code, "INSTANCE_CREATE_FAILED");
        assert!(f.root.is_dir());
        assert!(!f.root.join("demo").exists());
        let mut value = f.input();
        value["id"] = json!([255]);
        assert_eq!(
            f.request(value).unwrap_err(),
            invalid("the instance id is invalid")
        );
        f.sentinels();
    }
    #[test]
    fn anchored_writer_matches_string_api_and_cleans_encode_failure() {
        use super::super::{Manifest, Root};
        let f = Fixture::new();
        let a = Root::open_native(f.owner.path().join("old").as_os_str()).unwrap();
        let b = Root::open_native(f.owner.path().join("new").as_os_str()).unwrap();
        let m = Manifest {
            schema_version: 1,
            id: "demo".into(),
            name: "Name".into(),
            version_id: "v".into(),
            fabric_loader: None,
            neoforge_version: None,
            forge_version: None,
            source: "official".into(),
        };
        a.create(&m).unwrap();
        b.create_with(&m.id, || Ok(m.encode().unwrap())).unwrap();
        let expected = m.encode().unwrap();
        for directory in ["old", "new"] {
            assert_eq!(
                std::fs::read(f.owner.path().join(directory).join("demo/instance.json")).unwrap(),
                expected
            );
        }
        assert_eq!(
            a.create(&m).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            b.create_with(&m.id, || panic!("existing instance must not encode"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert!(
            b.create_with("broken", || Err(std::io::Error::other(
                "synthetic encode failure"
            )))
            .is_err()
        );
        assert!(!f.owner.path().join("new/broken").exists());
        f.sentinels();
    }
    #[cfg(unix)]
    #[test]
    fn guarded_false_native_byte_root_and_nofollow_are_bounded() {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::symlink;
        let mut f = Fixture::new();
        f.root = f
            .owner
            .path()
            .join(std::ffi::OsStr::from_bytes(b"root-\xff"));
        let mut value = input();
        value["directory"] = json!(f.root.as_os_str().as_bytes());
        value["version_id"] = json!([255]);
        symlink(f.owner.path(), &f.root).unwrap();
        assert_eq!(
            f.request(value.clone()).unwrap_err().code,
            "INSTANCES_ROOT_ERROR"
        );
        assert!(!f.owner.path().join("demo").exists());
        std::fs::remove_file(&f.root).unwrap();
        f.request(value.clone()).unwrap();
        let bytes = std::fs::read(f.root.join("demo/instance.json")).unwrap();
        assert_eq!(
            bytes,
            prepare(Some(&raw(&value)))
                .unwrap()
                .manifest
                .encode()
                .unwrap()
        );
        symlink(f.owner.path(), f.root.join("linked")).unwrap();
        value["id"] = json!("linked");
        assert_eq!(f.request(value).unwrap_err().code, "INSTANCE_EXISTS");
        assert!(!f.owner.path().join("instance.json").exists());
        f.sentinels();
    }
}

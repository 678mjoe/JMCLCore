use crate::protocol::RpcError;
use serde_json::{Value, value::RawValue};
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    if method != "server.create" {
        return None;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = params;
        Some(Err(RpcError::new(
            "UNSUPPORTED_PLATFORM",
            "Server management is currently supported only on Linux",
        )))
    }
    #[cfg(target_os = "linux")]
    {
        Some((|| {
            let p = linux::parse(params)?;
            let transport = crate::version::HttpTransport::new().map_err(|_| {
                RpcError::new(
                    "VERSION_RESOLUTION_FAILED",
                    "Failed to resolve the Minecraft version and loader",
                )
            })?;
            let service = crate::version::Service::new(transport, ".jmclcore/metadata-cache");
            linux::create(p, &service)
        })())
    }
}
#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use crate::servers::{manifest::Manifest, planning::Planner, store};
    use serde::Deserialize;
    fn official() -> String {
        "official".into()
    }
    #[derive(Deserialize)]
    pub struct Params {
        pub directory: String,
        pub id: String,
        #[serde(default)]
        pub name: Option<String>,
        pub version_id: String,
        #[serde(default)]
        pub fabric_loader: Option<String>,
        #[serde(default)]
        pub neoforge_version: Option<String>,
        #[serde(default)]
        pub forge_version: Option<String>,
        #[serde(default = "official")]
        pub source: String,
        #[serde(default)]
        pub accept_eula: bool,
        #[serde(default)]
        pub java_path: Option<String>,
    }
    fn invalid(m: &str) -> RpcError {
        RpcError::new("INVALID_PARAMS", m)
    }
    pub fn parse(raw: Option<&RawValue>) -> Result<Params, RpcError> {
        let raw =
            raw.ok_or_else(|| invalid("server creation requires directory, id, and version_id"))?;
        let value = crate::servers::params::object(
            raw,
            &["directory", "id", "version_id", "source"],
            &[
                "name",
                "fabric_loader",
                "neoforge_version",
                "forge_version",
                "java_path",
            ],
            &[],
        )
        .map_err(|_| invalid("server creation parameters are invalid"))?;
        let p: Params = serde_json::from_value(value)
            .map_err(|_| invalid("server creation parameters are invalid"))?;
        if p.directory.is_empty() || p.id.is_empty() || p.version_id.is_empty() {
            return Err(invalid("server creation parameters are out of range"));
        }
        crate::instances::validate_id(&p.id).map_err(|_| invalid("the server id is invalid"))?;
        crate::metadata::Source::parse(&p.source)
            .ok_or_else(|| invalid("source must be official or bmclapi"))?;
        if [&p.fabric_loader, &p.neoforge_version, &p.forge_version]
            .iter()
            .filter(|v| v.is_some())
            .count()
            > 1
        {
            return Err(invalid("only one loader field may be set"));
        }
        Ok(p)
    }
    pub fn create(p: Params, planner: &impl Planner) -> Result<Value, RpcError> {
        let m = Manifest {
            schema_version: 1,
            id: p.id.clone(),
            name: p.name.unwrap_or(p.id),
            version_id: p.version_id,
            fabric_loader: p.fabric_loader,
            neoforge_version: p.neoforge_version,
            forge_version: p.forge_version,
            source: p.source,
            accept_eula: p.accept_eula,
            java_path: p.java_path,
        };
        planner.resolve(&m).map_err(|_| {
            RpcError::new(
                "VERSION_RESOLUTION_FAILED",
                "Failed to resolve the Minecraft version and loader",
            )
        })?;
        let root = store::open_root(&p.directory)
            .map_err(|_| RpcError::new("SERVERS_ROOT_ERROR", "Failed to open the servers root"))?;
        m.validate()
            .map_err(|_| invalid("server creation parameters are out of range"))?;
        rustix::fs::mkdirat(&root, m.id.as_str(), rustix::fs::Mode::from_raw_mode(0o755)).map_err(
            |e| {
                if e == rustix::io::Errno::EXIST {
                    RpcError::new("SERVER_EXISTS", "A server with this id already exists")
                } else {
                    RpcError::new("SERVER_CREATE_FAILED", "Failed to create the server")
                }
            },
        )?;
        let result =
            store::open_server(&root, &m.id).and_then(|dir| store::write_manifest(&dir, &m));
        if result.is_err() {
            let _ = rustix::fs::unlinkat(&root, m.id.as_str(), rustix::fs::AtFlags::REMOVEDIR);
            return Err(RpcError::new(
                "SERVER_CREATE_FAILED",
                "Failed to create the server",
            ));
        }
        Ok(
            serde_json::json!({"stage":"server.create","id":m.id,"name":m.name,"version_id":m.version_id,"fabric_loader":m.fabric_loader,"neoforge_version":m.neoforge_version,"forge_version":m.forge_version,"source":m.source,"accept_eula":m.accept_eula,"java_path":m.java_path,"installed":false}),
        )
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::*;
    use crate::{
        metadata::Error,
        plan::{Plan, Target},
        servers::{manifest::Manifest, planning::Planner, store},
    };
    struct Stub(bool);
    impl Planner for Stub {
        fn resolve(&self, _: &Manifest) -> Result<(), Error> {
            if self.0 { Ok(()) } else { Err(Error::Network) }
        }
        fn plan(&self, _: &Manifest, _: Target) -> Result<Plan, Error> {
            unreachable!()
        }
    }
    fn params(root: &std::path::Path) -> Params {
        serde_json::from_value(
            serde_json::json!({"directory":root,"id":"demo","version_id":"1.21.4"}),
        )
        .unwrap()
    }
    #[test]
    fn create_resolves_before_disk_and_preserves_existing_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("servers");
        assert_eq!(
            create(params(&root), &Stub(false)).unwrap_err().code,
            "VERSION_RESOLUTION_FAILED"
        );
        assert!(!root.exists());
        let result = create(params(&root), &Stub(true)).unwrap();
        assert_eq!(result["installed"], false);
        assert_eq!(result["accept_eula"], false);
        let original = std::fs::read(root.join("demo/server.json")).unwrap();
        assert_eq!(
            create(params(&root), &Stub(true)).unwrap_err().code,
            "SERVER_EXISTS"
        );
        assert_eq!(
            std::fs::read(root.join("demo/server.json")).unwrap(),
            original
        );
        assert!(!root.join("demo/server.jar").exists());
        let dir = store::open_root(root.to_str().unwrap()).unwrap();
        assert_eq!(store::load(&dir, "demo").unwrap().name, "demo");
    }
    #[test]
    fn invalid_manifest_fields_follow_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let mut p = params(temp.path());
        p.name = Some(String::new());
        assert_eq!(
            create(p, &Stub(false)).unwrap_err().code,
            "VERSION_RESOLUTION_FAILED"
        );
        let mut p = params(temp.path());
        p.name = Some(String::new());
        assert_eq!(create(p, &Stub(true)).unwrap_err().code, "INVALID_PARAMS");
        assert!(!temp.path().join("demo").exists());
    }
}

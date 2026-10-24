//! Server installation holds the stopped guard through verified materialization,
//! private configuration writes, loader processors and the final schema-3 marker.
use crate::protocol;
#[cfg(target_os = "linux")]
use crate::protocol::RpcError;
use serde_json::value::RawValue;
use std::io::{self, Write};
pub fn handle(
    out: &mut impl Write,
    id: &str,
    method: &str,
    raw: Option<&RawValue>,
) -> Option<io::Result<()>> {
    if method != "server.install" {
        return None;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = raw;
        Some(protocol::error(
            out,
            Some(id),
            "UNSUPPORTED_PLATFORM",
            "Server management is currently supported only on Linux",
        ))
    }
    #[cfg(target_os = "linux")]
    {
        Some((|| {
            let p = match linux::parse(raw) {
                Ok(p) => p,
                Err(e) => return failure(out, id, e),
            };
            let metadata = match crate::version::HttpTransport::new() {
                Ok(t) => t,
                Err(_) => {
                    return failure(
                        out,
                        id,
                        linux::planning_error(crate::metadata::Error::Network),
                    );
                }
            };
            let planner = crate::version::Service::new(metadata, ".jmclcore/metadata-cache");
            let transport = match crate::install::HttpTransport::new() {
                Ok(t) => t,
                Err(_) => return failure(out, id, linux::install_error()),
            };
            linux::run(
                out,
                id,
                p,
                &planner,
                &transport,
                Some(&std::env::vars_os().collect()),
            )
        })())
    }
}
#[cfg(target_os = "linux")]
fn failure(out: &mut impl Write, id: &str, e: RpcError) -> io::Result<()> {
    protocol::error(out, Some(id), &e.code, &e.message)
}
#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use crate::{
        install,
        java::detect,
        metadata,
        plan::{Plan, Target},
        servers::{
            lifecycle,
            manifest::Manifest,
            planning::{self, Planner},
            properties, store,
        },
        store::{ArtifactRoot, Store},
    };
    use serde::Deserialize;
    use serde_json::{Value, json};
    use sha1::{Digest, Sha1};
    use std::{fs::File, io::Read};
    fn err(c: &str, m: &str) -> RpcError {
        RpcError::new(c, m)
    }
    fn workers() -> u8 {
        8
    }
    fn retries() -> u8 {
        2
    }
    fn store_directory() -> String {
        ".jmclcore".into()
    }
    fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
        use serde::de::Error;
        u8::try_from(super::super::numbers::u64_number(d)?).map_err(D::Error::custom)
    }
    #[derive(Deserialize)]
    pub struct Params {
        pub directory: String,
        pub id: String,
        #[serde(default)]
        pub accept_eula: bool,
        #[serde(default = "store_directory")]
        pub store_directory: String,
        #[serde(default = "workers", deserialize_with = "number")]
        pub workers: u8,
        #[serde(default = "retries", deserialize_with = "number")]
        pub retries: u8,
        #[serde(default)]
        pub java_paths: Vec<String>,
    }
    pub fn parse(raw: Option<&RawValue>) -> Result<Params, RpcError> {
        let raw = raw.ok_or_else(|| {
            err(
                "INVALID_PARAMS",
                "server installation requires directory, id, and EULA acceptance",
            )
        })?;
        let value = crate::servers::params::object(
            raw,
            &["directory", "id", "store_directory"],
            &[],
            &["java_paths"],
        )
        .map_err(|_| {
            err(
                "INVALID_PARAMS",
                "server installation parameters are invalid",
            )
        })?;
        let p: Params = serde_json::from_value(value).map_err(|_| {
            err(
                "INVALID_PARAMS",
                "server installation parameters are invalid",
            )
        })?;
        if p.directory.is_empty()
            || p.id.is_empty()
            || p.store_directory.is_empty()
            || p.workers == 0
            || p.workers > 32
            || p.retries > 5
        {
            return Err(err(
                "INVALID_PARAMS",
                "server installation parameters are out of range",
            ));
        }
        if !p.accept_eula {
            return Err(err(
                "EULA_NOT_ACCEPTED",
                "Server installation requires explicit Minecraft EULA acceptance",
            ));
        }
        Ok(p)
    }
    pub fn planning_error(e: metadata::Error) -> RpcError {
        match e.name() {
            "ServerDownloadMissing" => err(
                "SERVER_DOWNLOAD_UNAVAILABLE",
                "The selected Minecraft version has no dedicated server download",
            ),
            "UnsupportedServerLoader" => err(
                "SERVER_LOADER_UNSUPPORTED",
                "This server loader is not supported yet",
            ),
            _ => err(
                "SERVER_PLAN_FAILED",
                "Failed to create the server installation plan",
            ),
        }
    }
    pub fn install_error() -> RpcError {
        err(
            "SERVER_INSTALL_FAILED",
            "Failed to download and materialize the server jar",
        )
    }
    pub struct Prepared {
        pub manifest: Manifest,
        pub plan: Plan,
        pub runtime: detect::Runtime,
        pub root: ArtifactRoot,
        pub(super) dir: File,
        pub store: Store,
        pub options: install::Options,
        pub(super) _guard: lifecycle::StoppedGuard,
    }
    pub fn prepare(
        p: Params,
        planner: &impl Planner,
        env: Option<&detect::Environment>,
    ) -> Result<Prepared, RpcError> {
        prepare_detailed(p, planner, env, &mut None)
    }
    pub fn prepare_detailed(
        p: Params,
        planner: &impl Planner,
        env: Option<&detect::Environment>,
        detail: &mut Option<String>,
    ) -> Result<Prepared, RpcError> {
        let root = store::open_root(&p.directory).map_err(|e| {
            *detail = Some(format!(
                "failed to open servers directory: {}",
                super::super::creation_cli::io_name(&e)
            ));
            err("SERVERS_ROOT_ERROR", "Failed to open the servers root")
        })?;
        let mut manifest = store::load(&root, &p.id).map_err(|e| {
            *detail = Some(format!(
                "failed to load server: {}",
                super::super::creation_cli::io_name(&e)
            ));
            if e.kind() == io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
                {
                    *detail = Some("no server matches the given id".into());
                    err("SERVER_NOT_FOUND", "No server matches the given id")
                }
            } else {
                err("SERVER_LOAD_FAILED", "Failed to load the server")
            }
        })?;
        let dir = store::open_server(&root, &manifest.id).map_err(|e| {
            *detail = Some(format!(
                "failed to open server directory: {}",
                super::super::creation_cli::io_name(&e)
            ));
            err("SERVER_ROOT_ERROR", "Failed to open the server directory")
        })?;
        let guard = lifecycle::lock_stopped(&dir).map_err(|e| {
            *detail = Some(if e == "ServerAlreadyRunning" {
                "stop the server before installing it".into()
            } else {
                format!("failed to validate the server lifecycle status: {e}")
            });
            if e == "ServerAlreadyRunning" {
                err(
                    "SERVER_ALREADY_RUNNING",
                    "Stop the server before installing it",
                )
            } else {
                err(
                    "SERVER_STATUS_FAILED",
                    "Failed to validate the server lifecycle status",
                )
            }
        })?;
        if !manifest.accept_eula {
            manifest.accept_eula = true;
            store::write_manifest(&dir, &manifest).map_err(|e| {
                *detail = Some(format!(
                    "failed to record Minecraft EULA acceptance: {}",
                    super::super::creation_cli::io_name(&e)
                ));
                err(
                    "SERVER_STATE_ERROR",
                    "Failed to record Minecraft EULA acceptance",
                )
            })?;
        }
        let target = Target::host().map_err(|e| {
            *detail = Some(format!("unsupported host platform: {}", e.name()));
            err("UNSUPPORTED_PLATFORM", "The host platform is not supported")
        })?;
        let plan = planner.plan(&manifest, target).map_err(|e| {
            *detail = Some(match e.name() {
                "ServerDownloadMissing" => {
                    "the selected Minecraft version has no dedicated server download".into()
                }
                "UnsupportedServerLoader" => "This server loader is not supported yet".into(),
                _ => format!(
                    "failed to create the server installation plan: {}",
                    e.name()
                ),
            });
            planning_error(e)
        })?;
        let environment = env.ok_or_else(|| {
            err(
                "ENVIRONMENT_UNAVAILABLE",
                "Process environment is unavailable",
            )
        })?;
        let paths = manifest
            .java_path
            .as_ref()
            .map(|p| vec![p.clone()])
            .unwrap_or(p.java_paths);
        let detection = detect::detect(environment, &paths).map_err(|_| {
            *detail = Some("no Java executable candidates were found: JavaNotFound".into());
            err("JAVA_NOT_FOUND", "No Java executable candidates were found")
        })?;
        let runtime =
            detect::select_compatible(&detection, plan.java_major_version, &plan.target.arch)
                .map_err(|_| {
                    err(
                        "JAVA_INCOMPATIBLE",
                        "No Java runtime matches the server requirement",
                    )
                })?;
        let store = Store::open(p.store_directory).map_err(|e| {
            *detail = Some(format!(
                "failed to open artifact store: {}",
                super::super::creation_cli::io_name(&e)
            ));
            err("STORE_ROOT_ERROR", "Failed to open the artifact store")
        })?;
        let root = ArtifactRoot::from_directory(dir.try_clone().map_err(|e| {
            *detail = Some(format!(
                "failed to open server directory: {}",
                super::super::creation_cli::io_name(&e)
            ));
            err("SERVER_ROOT_ERROR", "Failed to open the server directory")
        })?)
        .map_err(|e| {
            *detail = Some(format!(
                "failed to open server directory: {}",
                super::super::creation_cli::io_name(&e)
            ));
            err("SERVER_ROOT_ERROR", "Failed to open the server directory")
        })?;
        Ok(Prepared {
            manifest,
            plan,
            runtime,
            root,
            dir,
            store,
            options: install::Options {
                workers: p.workers,
                retries: p.retries,
                sync_files: true,
            },
            _guard: guard,
        })
    }
    fn digest(mut file: File) -> io::Result<(String, u64)> {
        let mut hash = Sha1::new();
        let mut buffer = [0; 32768];
        let mut size = 0;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            size += n as u64;
        }
        Ok((format!("{:x}", hash.finalize()), size))
    }
    pub fn execute<T: install::Transport>(
        p: &Prepared,
        transport: &T,
        env: Option<&detect::Environment>,
        sink: &(dyn Fn(install::Event) -> Result<(), install::Error> + Sync),
    ) -> Result<Value, RpcError> {
        let summary =
            install::execute(&p.root, &p.store, &p.plan, p.options, transport, Some(sink))
                .map_err(|_| install_error())?;
        p.root
            .atomic_replace("eula.txt", properties::EULA_CONTENT.as_bytes())
            .map_err(|_| err("SERVER_CONFIGURATION_FAILED", "Failed to write eula.txt"))?;
        let properties_created =
            match rustix::fs::statat(&p.dir, "server.properties", rustix::fs::AtFlags::empty()) {
                Ok(stat) => {
                    if rustix::fs::FileType::from_raw_mode(stat.st_mode)
                        != rustix::fs::FileType::RegularFile
                    {
                        return Err(err(
                            "SERVER_CONFIGURATION_FAILED",
                            "Failed to initialize server.properties",
                        ));
                    }
                    false
                }
                Err(rustix::io::Errno::NOENT) => {
                    p.root
                        .atomic_replace("server.properties", properties::DEFAULT_CONTENT.as_bytes())
                        .map_err(|_| {
                            err(
                                "SERVER_CONFIGURATION_FAILED",
                                "Failed to initialize server.properties",
                            )
                        })?;
                    true
                }
                Err(_) => {
                    return Err(err(
                        "SERVER_CONFIGURATION_FAILED",
                        "Failed to initialize server.properties",
                    ));
                }
            };
        let server = p
            .plan
            .files
            .first()
            .filter(|f| f.path == "server.jar")
            .ok_or_else(install_error)?;
        let mut launch_file = "server.jar".to_owned();
        let mut launch_mode = store::LaunchMode::Jar;
        let mut launch_sha1 = server.sha1.clone();
        let mut launch_size = server.size;
        match p.plan.loader.as_str() {
            "fabric" => {
                if p.manifest.fabric_loader.is_none() {
                    return Err(install_error());
                }
                let args = planning::fabric_args(&p.plan.files).map_err(|_| install_error())?;
                launch_file = ".jmcl/fabric_args.txt".into();
                p.root
                    .atomic_replace(&launch_file, &args)
                    .map_err(|_| install_error())?;
                launch_sha1 = format!("{:x}", Sha1::digest(&args));
                launch_size = args.len() as u64;
                launch_mode = store::LaunchMode::ArgsFile;
            }
            "forge" | "neoforge" => {
                use crate::loaders::{self, Loader};
                let loader = if p.plan.loader == "neoforge" {
                    let version = p
                        .manifest
                        .neoforge_version
                        .as_deref()
                        .ok_or_else(install_error)?;
                    launch_file =
                        format!("libraries/net/neoforged/neoforge/{version}/unix_args.txt");
                    Loader::NeoForge { version }
                } else {
                    let version = p
                        .manifest
                        .forge_version
                        .as_deref()
                        .ok_or_else(install_error)?;
                    launch_file = format!(
                        "libraries/net/minecraftforge/forge/{}-{version}/unix_args.txt",
                        p.manifest.version_id
                    );
                    Loader::Forge {
                        minecraft: &p.manifest.version_id,
                        version,
                    }
                };
                loaders::run(
                    &p.root,
                    &p.store,
                    loader,
                    &p.plan.version_id,
                    &loaders::Options {
                        server: true,
                        java_paths: vec![p.runtime.executable.clone()],
                        required_major: p.plan.java_major_version,
                        minecraft_jar_relative: None,
                    },
                    env,
                    &mut |_| Ok(()),
                )
                .map_err(|_| install_error())?;
                (launch_sha1, launch_size) = digest(
                    p.root
                        .open_file(&launch_file)
                        .map_err(|_| install_error())?,
                )
                .map_err(|_| install_error())?;
                launch_mode = store::LaunchMode::ArgsFile;
            }
            _ => {}
        }
        let marker = store::Installation {
            schema_version: 3,
            version_id: p.manifest.version_id.clone(),
            fabric_loader: p.manifest.fabric_loader.clone(),
            neoforge_version: p.manifest.neoforge_version.clone(),
            forge_version: p.manifest.forge_version.clone(),
            source: p.manifest.source.clone(),
            server_sha1: server.sha1.clone(),
            server_size: server.size,
            java_major_version: p.plan.java_major_version,
            launch_mode,
            launch_file,
            launch_sha1,
            launch_size,
        };
        if !marker.valid() {
            return Err(err(
                "SERVER_STATE_ERROR",
                "Failed to record completed server installation",
            ));
        }
        let mut bytes = serde_json::to_vec(&marker).map_err(|_| {
            err(
                "SERVER_STATE_ERROR",
                "Failed to record completed server installation",
            )
        })?;
        bytes.push(b'\n');
        p.root
            .atomic_replace(".jmcl/install.json", &bytes)
            .map_err(|_| {
                err(
                    "SERVER_STATE_ERROR",
                    "Failed to record completed server installation",
                )
            })?;
        let mut result = serde_json::to_value(summary).unwrap();
        let object = result.as_object_mut().unwrap();
        for (k, v) in [
            ("stage", json!("server.install")),
            ("id", json!(p.manifest.id)),
            ("version_id", json!(p.manifest.version_id)),
            ("source", json!(p.manifest.source)),
            ("java", json!(p.runtime)),
            ("properties_created", json!(properties_created)),
        ] {
            object.insert(k.into(), v);
        }
        Ok(result)
    }
    pub fn run<P: Planner, T: install::Transport>(
        out: &mut impl Write,
        id: &str,
        p: Params,
        planner: &P,
        transport: &T,
        env: Option<&detect::Environment>,
    ) -> io::Result<()> {
        let p = match prepare(p, planner, env) {
            Ok(p) => p,
            Err(e) => return failure(out, id, e),
        };
        serde_json::to_writer(
            &mut *out,
            &json!({"protocol":1,"id":id,"event":"started","source":p.manifest.source,"started":{"stage":"download","version_id":p.plan.version_id,"files_total":p.plan.files.len(),"bytes_total":p.plan.total_download_size}}),
        )?;
        writeln!(out)?;
        out.flush()?;
        let result = install::stream::stream(
            |sink| execute(&p, transport, env, sink),
            |event| install::rpc::write_event(out, id, p.plan.source, event),
        )?;
        match result {
            Ok(result) => {
                serde_json::to_writer(
                    &mut *out,
                    &json!({"protocol":1,"id":id,"event":"result","result":result}),
                )?;
                writeln!(out)
            }
            Err(e) => failure(out, id, e),
        }
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::*;
    use crate::{
        install,
        java::detect,
        metadata::{Error, Source},
        plan::{self, Plan, Target},
        servers::{lifecycle, manifest::Manifest, planning::Planner, store},
        store::{ArtifactRoot, Store},
    };
    use serde_json::{Value, json};
    use sha1::{Digest, Sha1};
    use std::{fs, io::Write, os::unix::fs::MetadataExt};
    struct Bytes(bool);
    impl install::Transport for Bytes {
        fn fetch(&self, _: &str, sink: &mut dyn Write) -> Result<u16, install::Error> {
            sink.write_all(if self.0 { b"jar" } else { b"bad" })?;
            Ok(200)
        }
    }
    struct Missing;
    impl Planner for Missing {
        fn resolve(&self, _: &Manifest) -> Result<(), Error> {
            Ok(())
        }
        fn plan(&self, _: &Manifest, _: Target) -> Result<Plan, Error> {
            Err(Error::Plan("ServerDownloadMissing"))
        }
    }
    fn fixture(path: &std::path::Path, fabric: bool) -> Prepared {
        let dir = store::open_root(path.to_str().unwrap()).unwrap();
        let manifest:Manifest=serde_json::from_value(json!({"id":"demo","name":"Demo","version_id":"fixture","accept_eula":true,"fabric_loader":if fabric{Some("0.16.10")}else{None}})).unwrap();
        store::write_manifest(&dir, &manifest).unwrap();
        let guard = lifecycle::lock_stopped(&dir).unwrap();
        let hash = format!("{:x}", Sha1::digest(b"jar"));
        let metadata = json!({"id":"fixture","downloads":{"server":{"url":"https://fixture.invalid/server.jar","sha1":hash,"size":3}},"javaVersion":{"majorVersion":21}});
        let mut plan =
            plan::build_server(&metadata, Source::Official, Target::host().unwrap(), &[]).unwrap();
        if fabric {
            plan.loader = "fabric".into()
        }
        let runtime = detect::parse_output(
            "/fixture/java",
            detect::CandidateSource::Explicit,
            "java.specification.version = 21\nos.arch = amd64\n",
            "",
        )
        .unwrap();
        Prepared {
            manifest,
            plan,
            runtime,
            root: ArtifactRoot::from_directory(dir.try_clone().unwrap()).unwrap(),
            dir,
            store: Store::open(path.join("store")).unwrap(),
            options: install::Options {
                workers: 1,
                retries: 0,
                sync_files: true,
            },
            _guard: guard,
        }
    }
    #[test]
    fn hardlinked_jar_private_configuration_and_marker_last() {
        let temp = tempfile::tempdir().unwrap();
        let p = fixture(temp.path(), false);
        let result = execute(&p, &Bytes(true), None, &|_| Ok(())).unwrap();
        assert_eq!(result["files_hard_linked"], 1);
        assert_eq!(result["properties_created"], true);
        assert!(
            fs::metadata(temp.path().join("server.jar"))
                .unwrap()
                .nlink()
                >= 2
        );
        assert_eq!(
            fs::metadata(temp.path().join("eula.txt")).unwrap().nlink(),
            1
        );
        let record = store::load_installation(&p.dir).unwrap().unwrap();
        assert!(record.valid());
        assert_eq!(record.launch_file, "server.jar");
        fs::write(temp.path().join("server.properties"), b"custom bytes\n").unwrap();
        let result = execute(&p, &Bytes(false), None, &|_| Ok(())).unwrap();
        assert_eq!(result["files_cached"], 1);
        assert_eq!(result["properties_created"], false);
        assert_eq!(
            fs::read(temp.path().join("server.properties")).unwrap(),
            b"custom bytes\n"
        );
    }
    #[test]
    fn fabric_args_are_private_and_hashed_in_completed_marker() {
        let temp = tempfile::tempdir().unwrap();
        let p = fixture(temp.path(), true);
        execute(&p, &Bytes(true), None, &|_| Ok(())).unwrap();
        let record = store::load_installation(&p.dir).unwrap().unwrap();
        assert_eq!(record.launch_file, ".jmcl/fabric_args.txt");
        let bytes = fs::read(temp.path().join(&record.launch_file)).unwrap();
        assert_eq!(
            bytes,
            b"-cp :server.jar\nnet.fabricmc.loader.impl.launch.knot.KnotServer\n"
        );
        assert_eq!(record.launch_sha1, format!("{:x}", Sha1::digest(&bytes)));
        assert_eq!(record.launch_size, bytes.len() as u64);
        assert_eq!(
            fs::metadata(temp.path().join(&record.launch_file))
                .unwrap()
                .nlink(),
            1
        );
    }
    #[test]
    fn bad_download_or_configuration_never_marks_complete() {
        for bad_download in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let p = fixture(temp.path(), false);
            if !bad_download {
                fs::create_dir(temp.path().join("server.properties")).unwrap()
            }
            let e = execute(&p, &Bytes(!bad_download), None, &|_| Ok(())).unwrap_err();
            assert_eq!(
                e.code,
                if bad_download {
                    "SERVER_INSTALL_FAILED"
                } else {
                    "SERVER_CONFIGURATION_FAILED"
                }
            );
            assert!(!temp.path().join(".jmcl/install.json").exists());
            assert_eq!(temp.path().join("eula.txt").exists(), !bad_download);
        }
    }
    #[test]
    fn missing_processor_inputs_never_report_completed_installation() {
        let temp = tempfile::tempdir().unwrap();
        let mut p = fixture(temp.path(), false);
        p.plan.loader = "forge".into();
        p.manifest.forge_version = Some("49.2.8".into());
        assert_eq!(
            execute(&p, &Bytes(true), None, &|_| Ok(()))
                .unwrap_err()
                .code,
            "SERVER_INSTALL_FAILED"
        );
        assert!(temp.path().join("eula.txt").exists());
        assert!(!temp.path().join(".jmcl/install.json").exists());
    }
    #[test]
    fn writer_failure_prevents_configuration_and_marker() {
        let temp = tempfile::tempdir().unwrap();
        let p = fixture(temp.path(), false);
        assert!(
            execute(&p, &Bytes(true), None, &|_| Err(install::Error::named(
                "WriteFailed"
            )))
            .is_err()
        );
        assert!(!temp.path().join("eula.txt").exists());
        assert!(!temp.path().join(".jmcl/install.json").exists());
    }
    #[test]
    fn acceptance_is_recorded_before_planning_but_not_before_explicit_consent() {
        let temp = tempfile::tempdir().unwrap();
        let servers = temp.path().join("servers");
        fs::create_dir_all(servers.join("demo")).unwrap();
        let manifest = json!({"id":"demo","name":"Demo","version_id":"fixture"});
        fs::write(servers.join("demo/server.json"), manifest.to_string()).unwrap();
        let value = json!({"directory":servers,"id":"demo","accept_eula":false});
        let raw = serde_json::value::to_raw_value(&value).unwrap();
        assert_eq!(parse(Some(&raw)).err().unwrap().code, "EULA_NOT_ACCEPTED");
        let mut value = value;
        value["accept_eula"] = true.into();
        let raw = serde_json::value::to_raw_value(&value).unwrap();
        assert_eq!(
            prepare(parse(Some(&raw)).unwrap(), &Missing, None)
                .err()
                .unwrap()
                .code,
            "SERVER_DOWNLOAD_UNAVAILABLE"
        );
        let now: Value =
            serde_json::from_slice(&fs::read(servers.join("demo/server.json")).unwrap()).unwrap();
        assert_eq!(now["accept_eula"], true);
        assert!(!servers.join("demo/eula.txt").exists());
        assert!(!servers.join("demo/.jmcl/install.json").exists());
    }
}

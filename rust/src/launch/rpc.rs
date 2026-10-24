//! Secure launch.plan request decoding. Unknown fields stay borrowed RawValue;
//! decoded strings always have wiping owners, never a generic JSON tree.
use super::{Auth, Options, Plan};
use crate::{
    auth::json::Json,
    java::{detect, managed, select},
    metadata::{self, Source},
    plan,
    protocol::{self, RpcError},
    version::{self, Service, Transport},
};
use serde::Serialize;
use serde_json::value::RawValue;
use std::{
    collections::HashMap,
    io::{self, Write},
};
use zeroize::Zeroizing;
type Text = Zeroizing<String>;
fn string(j: Json<'_>, k: &str, default: Option<&str>) -> Result<Text, ()> {
    match j.field(k)? {
        Some(v) => v.string(),
        None => default.map(|s| Zeroizing::new(s.into())).ok_or(()),
    }
}
fn optional(j: Json<'_>, k: &str) -> Result<Option<Text>, ()> {
    j.field(k)?
        .filter(|v| !v.null())
        .map(Json::string)
        .transpose()
}
fn boolean(j: Json<'_>, k: &str, default: bool) -> Result<bool, ()> {
    match j.field(k)? {
        None => Ok(default),
        Some(v) => match v.0 {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(()),
        },
    }
}
fn number(j: Json<'_>, k: &str) -> Result<Option<u32>, ()> {
    j.field(k)?.filter(|v| !v.null()).map(Json::u32).transpose()
}
fn strings(j: Json<'_>, k: &str) -> Result<Vec<Text>, ()> {
    j.field(k)?
        .map(|v| v.array()?.into_iter().map(Json::string).collect())
        .unwrap_or(Ok(vec![]))
}
struct OwnedAuth {
    player_name: Text,
    uuid: Text,
    access_token: Text,
    session: Option<Text>,
    user_type: Text,
    xuid: Text,
    client_id: Text,
    user_properties: Text,
}
impl OwnedAuth {
    fn parse(j: Json<'_>) -> Result<Self, ()> {
        Ok(Self {
            player_name: string(j, "player_name", None)?,
            uuid: string(j, "uuid", None)?,
            access_token: string(j, "access_token", None)?,
            session: optional(j, "session")?,
            user_type: string(j, "user_type", Some("msa"))?,
            xuid: string(j, "xuid", Some(""))?,
            client_id: string(j, "client_id", Some(""))?,
            user_properties: string(j, "user_properties", Some("{}"))?,
        })
    }
    fn borrowed(&self) -> Auth<'_> {
        Auth {
            player_name: &self.player_name,
            uuid: &self.uuid,
            access_token: &self.access_token,
            session: self.session.as_ref().map(|s| s.as_str()),
            user_type: &self.user_type,
            xuid: &self.xuid,
            client_id: &self.client_id,
            user_properties: &self.user_properties,
        }
    }
}
struct OwnedOptions {
    demo: bool,
    width: Option<u32>,
    height: Option<u32>,
    quick_support: bool,
    include_default: bool,
    text: HashMap<&'static str, Text>,
    extra_jvm: Vec<Text>,
    extra_game: Vec<Text>,
}
impl OwnedOptions {
    fn parse(j: Json<'_>) -> Result<Self, ()> {
        let text = [
            ("quick_play_path", ""),
            ("quick_play_singleplayer", ""),
            ("quick_play_multiplayer", ""),
            ("quick_play_realms", ""),
            ("launcher_name", "jmcl-core"),
            ("launcher_version", "0.1.0-dev"),
        ]
        .into_iter()
        .map(|(k, v)| Ok((k, string(j, k, Some(v))?)))
        .collect::<Result<_, ()>>()?;
        Ok(Self {
            demo: boolean(j, "demo", false)?,
            width: number(j, "resolution_width")?,
            height: number(j, "resolution_height")?,
            quick_support: boolean(j, "quick_play_support", false)?,
            include_default: boolean(j, "include_default_user_jvm", true)?,
            text,
            extra_jvm: strings(j, "extra_jvm_args")?,
            extra_game: strings(j, "extra_game_args")?,
        })
    }
    fn borrowed<'a>(&'a self, jvm: &'a [&'a str], game: &'a [&'a str]) -> Options<'a> {
        Options {
            demo: self.demo,
            resolution_width: self.width,
            resolution_height: self.height,
            quick_play_support: self.quick_support,
            quick_play_path: &self.text["quick_play_path"],
            quick_play_singleplayer: &self.text["quick_play_singleplayer"],
            quick_play_multiplayer: &self.text["quick_play_multiplayer"],
            quick_play_realms: &self.text["quick_play_realms"],
            launcher_name: &self.text["launcher_name"],
            launcher_version: &self.text["launcher_version"],
            include_default_user_jvm: self.include_default,
            extra_jvm_args: jvm,
            extra_game_args: game,
        }
    }
}
struct Params {
    id: Text,
    directory: Text,
    natives: Text,
    auth: OwnedAuth,
    options: OwnedOptions,
    source: Source,
    arch: Option<Text>,
    fabric: Option<Text>,
    neo: Option<Text>,
    forge: Option<Text>,
    java_paths: Vec<Text>,
    java_policy: Text,
    java_override: Option<Text>,
    store: Text,
}
impl Params {
    fn parse(raw: &RawValue) -> Result<Self, ()> {
        let j = Json::parse(raw.get())?;
        let source = Source::parse(&string(j, "source", Some("official"))?).ok_or(())?;
        let arch = optional(j, "arch")?;
        if arch
            .as_ref()
            .is_some_and(|a| !matches!(a.as_str(), "x86" | "x86_64" | "arm" | "arm64"))
        {
            return Err(());
        }
        Ok(Self {
            id: string(j, "id", None)?,
            directory: string(j, "directory", Some(".minecraft"))?,
            natives: string(j, "natives_directory", None)?,
            auth: OwnedAuth::parse(j.required("auth")?)?,
            options: OwnedOptions::parse(j.field("options")?.unwrap_or(Json("{}")))?,
            source,
            arch,
            fabric: optional(j, "fabric_loader")?,
            neo: optional(j, "neoforge_version")?,
            forge: optional(j, "forge_version")?,
            java_paths: strings(j, "java_paths")?,
            java_policy: string(j, "java_policy", Some("auto"))?,
            java_override: optional(j, "java_override")?,
            store: string(j, "store_directory", Some(".jmclcore"))?,
        })
    }
}
fn invalid(s: &str) -> RpcError {
    RpcError::new("INVALID_PARAMS", s)
}
fn parse(raw: Option<&RawValue>) -> Result<Params, RpcError> {
    let raw =
        raw.ok_or_else(|| invalid("launch.plan requires version, paths, and authentication"))?;
    let p = Params::parse(raw).map_err(|_| invalid("launch.plan parameters are invalid"))?;
    if p.id.is_empty() || p.directory.is_empty() || p.natives.is_empty() {
        return Err(invalid(
            "launch.plan paths and version id must not be empty",
        ));
    }
    policy(&p.java_policy)?;
    if p.java_override.is_some() && !p.java_paths.is_empty() {
        return Err(invalid(
            "java_override is mutually exclusive with java_paths",
        ));
    }
    Ok(p)
}
fn policy(s: &str) -> Result<managed::Policy, RpcError> {
    match s {
        "auto" => Ok(managed::Policy::Auto),
        "local" => Ok(managed::Policy::Local),
        "managed" => Ok(managed::Policy::Managed),
        _ => Err(invalid("java_policy must be auto, local, or managed")),
    }
}
struct Resolved {
    p: Params,
    required: u16,
    java: managed::Resolution,
    launch: Plan,
}
fn resolve<T: Transport>(
    p: Params,
    service: &Service<T>,
    env: Option<&detect::Environment>,
) -> Result<Resolved, RpcError> {
    let mut target = plan::Target::host()
        .map_err(|_| RpcError::new("UNSUPPORTED_PLATFORM", "The host platform is not supported"))?;
    if let Some(a) = &p.arch {
        target.arch = a.to_string()
    }
    let m = if [&p.fabric, &p.neo, &p.forge]
        .iter()
        .filter(|x| x.is_some())
        .count()
        > 1
    {
        Err(metadata::Error::Plan("LoaderConflict"))
    } else if let Some(f) = &p.forge {
        service
            .resolve_forge(&p.id, f, p.source)
            .map(|r| r.metadata)
    } else if let Some(n) = &p.neo {
        service.resolve_neoforge(&p.id, n, p.source).map(|r| r.0)
    } else {
        service.resolve(&p.id, p.fabric.as_ref().map(|s| s.as_str()), p.source)
    }
    .map_err(|e| match e {
        metadata::Error::VersionNotFound => RpcError::new(
            "VERSION_NOT_FOUND",
            "The requested version is not in the selected manifest",
        ),
        _ => RpcError::new("METADATA_ERROR", "Failed to resolve version metadata"),
    })?;
    let assets =
        if !m["assetIndex"].is_null() {
            Some(service.assets(&m["assetIndex"], p.source).map_err(|_| {
                RpcError::new("ASSET_INDEX_ERROR", "Failed to resolve the asset index")
            })?)
        } else {
            None
        };
    let install = plan::build(&m, assets.as_ref(), p.source, target.clone()).map_err(|_| {
        RpcError::new(
            "INSTALL_PLAN_ERROR",
            "Failed to build the classpath and asset layout",
        )
    })?;
    let required = select::required_major(&m).map_err(|_| {
        RpcError::new(
            "INVALID_METADATA",
            "Version metadata contains an invalid Java requirement",
        )
    })?;
    let java = managed::resolve(
        env,
        &managed::Request {
            required_major: required,
            component: m["javaVersion"]["component"].as_str().map(Into::into),
            arch: target.arch,
            policy: policy(&p.java_policy)?,
            java_paths: p.java_paths.iter().map(|s| s.to_string()).collect(),
            java_override: p.java_override.as_ref().map(|s| s.to_string()),
            store_directory: p.store.to_string(),
            allow_download: false,
        },
        &mut |_, _| Err("JavaDownloadFailed"),
    )
    .map_err(|e| match e {
        "InvalidInput" => invalid("launch.plan Java parameters are invalid"),
        "EnvironmentUnavailable" => RpcError::new(
            "ENVIRONMENT_UNAVAILABLE",
            "Process environment is unavailable",
        ),
        "JavaNotFound" => {
            RpcError::new("JAVA_NOT_FOUND", "No Java executable candidates were found")
        }
        _ => RpcError::new(
            "JAVA_INCOMPATIBLE",
            "No Java runtime matches the required version and architecture",
        ),
    })?;
    let jvm = p
        .options
        .extra_jvm
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>();
    let game = p
        .options
        .extra_game
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>();
    let launch = super::build(super::Input {
        metadata: &m,
        install: &install,
        runtime: &java.runtime,
        root_directory: &p.directory,
        game_directory: &p.directory,
        natives_directory: &p.natives,
        auth: p.auth.borrowed(),
        options: p.options.borrowed(&jvm, &game),
        force_java: matches!(java.source, managed::Source::Forced),
    })
    .map_err(|e| match e {
        "InvalidAuthSession" => {
            RpcError::new("INVALID_AUTH", "Authentication fields must not be empty")
        }
        "JavaVersionIncompatible" | "JavaArchitectureIncompatible" => RpcError::new(
            "JAVA_INCOMPATIBLE",
            "The selected Java runtime does not match the required version and architecture",
        ),
        "UnknownPlaceholder" | "UnsupportedRulePattern" => RpcError::new(
            "UNSUPPORTED_METADATA",
            "Version metadata contains an unsupported launch expression",
        ),
        _ => RpcError::new("LAUNCH_PLAN_ERROR", "Failed to generate launch arguments"),
    })?;
    Ok(Resolved {
        p,
        required,
        java,
        launch,
    })
}
#[derive(Serialize)]
struct Summary<'a> {
    java_executable: &'a str,
    working_directory: &'a str,
    redacted_argv: Vec<&'a str>,
    sensitive_argv_indices: &'a [usize],
}
#[derive(Serialize)]
struct ResultBody<'a> {
    version_id: &'a str,
    required_java: u16,
    runtime: &'a detect::Runtime,
    java_source: managed::Source,
    forced: bool,
    launch: Summary<'a>,
}
#[derive(Serialize)]
struct Envelope<'a> {
    protocol: u32,
    id: &'a str,
    event: &'static str,
    source: Source,
    result: ResultBody<'a>,
}
fn write(out: &mut impl Write, id: &str, result: Result<Resolved, RpcError>) -> io::Result<()> {
    match result {
        Err(e) => protocol::error(out, Some(id), &e.code, &e.message),
        Ok(r) => {
            serde_json::to_writer(
                &mut *out,
                &Envelope {
                    protocol: 1,
                    id,
                    event: "result",
                    source: r.p.source,
                    result: ResultBody {
                        version_id: &r.p.id,
                        required_java: r.required,
                        runtime: &r.java.runtime,
                        java_source: r.java.source,
                        forced: matches!(r.java.source, managed::Source::Forced),
                        launch: Summary {
                            java_executable: r.launch.java_executable(),
                            working_directory: &r.launch.working_directory,
                            redacted_argv: r.launch.redacted_argv().collect(),
                            sensitive_argv_indices: &r.launch.sensitive_argv_indices,
                        },
                    },
                },
            )?;
            out.write_all(b"\n")
        }
    }
}
pub fn handle_with<T: Transport>(
    out: &mut impl Write,
    id: &str,
    raw: Option<&RawValue>,
    service: &Service<T>,
    env: Option<&detect::Environment>,
) -> io::Result<()> {
    write(out, id, parse(raw).and_then(|p| resolve(p, service, env)))
}
pub fn handle(
    out: &mut impl Write,
    id: &str,
    method: &str,
    raw: Option<&RawValue>,
) -> Option<io::Result<()>> {
    if method != "launch.plan" {
        return None;
    }
    let p = match parse(raw) {
        Ok(p) => p,
        Err(e) => return Some(protocol::error(out, Some(id), &e.code, &e.message)),
    };
    let transport = match version::HttpTransport::new() {
        Ok(t) => t,
        Err(_) => {
            return Some(protocol::error(
                out,
                Some(id),
                "METADATA_ERROR",
                "Failed to resolve version metadata",
            ));
        }
    };
    let service = Service::new(transport, ".jmclcore/metadata-cache");
    let env = std::env::vars_os().collect();
    Some(write(out, id, resolve(p, &service, Some(&env))))
}

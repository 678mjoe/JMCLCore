//! Native client launch planning. Authentication is borrowed, never placed in JSON Values.
pub mod rules;
use crate::{java::detect::Runtime, plan::Plan as InstallPlan};
use serde_json::Value;
use std::collections::HashMap;
use zeroize::Zeroizing;
pub type Error = &'static str;
pub struct Auth<'a> {
    pub player_name: &'a str,
    pub uuid: &'a str,
    pub access_token: &'a str,
    pub session: Option<&'a str>,
    pub user_type: &'a str,
    pub xuid: &'a str,
    pub client_id: &'a str,
    pub user_properties: &'a str,
}
pub struct Options<'a> {
    pub demo: bool,
    pub resolution_width: Option<u32>,
    pub resolution_height: Option<u32>,
    pub quick_play_support: bool,
    pub quick_play_path: &'a str,
    pub quick_play_singleplayer: &'a str,
    pub quick_play_multiplayer: &'a str,
    pub quick_play_realms: &'a str,
    pub launcher_name: &'a str,
    pub launcher_version: &'a str,
    pub include_default_user_jvm: bool,
    pub extra_jvm_args: &'a [&'a str],
    pub extra_game_args: &'a [&'a str],
}
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            demo: false,
            resolution_width: None,
            resolution_height: None,
            quick_play_support: false,
            quick_play_path: "",
            quick_play_singleplayer: "",
            quick_play_multiplayer: "",
            quick_play_realms: "",
            launcher_name: "jmcl-core",
            launcher_version: "0.1.0-dev",
            include_default_user_jvm: true,
            extra_jvm_args: &[],
            extra_game_args: &[],
        }
    }
}
pub struct Input<'a> {
    pub metadata: &'a Value,
    pub install: &'a InstallPlan,
    pub runtime: &'a Runtime,
    pub root_directory: &'a str,
    pub game_directory: &'a str,
    pub natives_directory: &'a str,
    pub auth: Auth<'a>,
    pub options: Options<'a>,
    pub force_java: bool,
}
/// Secret-bearing arguments have one owner. No Debug/Clone/Serialize implementation.
pub struct Plan {
    argv: Vec<Zeroizing<String>>,
    jvm_len: usize,
    pub working_directory: String,
    pub sensitive_argv_indices: Vec<usize>,
}
impl Plan {
    pub fn argv(&self) -> impl Iterator<Item = &str> {
        self.argv.iter().map(|s| s.as_str())
    }
    pub fn jvm_args(&self) -> impl Iterator<Item = &str> {
        self.argv[1..1 + self.jvm_len].iter().map(|s| s.as_str())
    }
    pub fn game_args(&self) -> impl Iterator<Item = &str> {
        self.argv[2 + self.jvm_len..].iter().map(|s| s.as_str())
    }
    pub fn main_class(&self) -> &str {
        &self.argv[1 + self.jvm_len]
    }
    pub fn java_executable(&self) -> &str {
        &self.argv[0]
    }
    pub fn redacted_argv(&self) -> impl Iterator<Item = &str> {
        self.argv.iter().enumerate().map(|(i, s)| {
            if self.sensitive_argv_indices.contains(&i) {
                "<redacted>"
            } else {
                s.as_str()
            }
        })
    }
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn join(root: &str, tail: &str) -> String {
    if root.is_empty() {
        return tail.into();
    }
    if tail.is_empty() {
        return root.into();
    }
    let sep = |b: u8| b == b'/' || (cfg!(windows) && b == b'\\');
    let (a, b) = (
        sep(*root.as_bytes().last().unwrap()),
        sep(tail.as_bytes()[0]),
    );
    if a && b {
        format!("{}{}", root, &tail[1..])
    } else if !a && !b {
        format!("{}{}{}", root, std::path::MAIN_SEPARATOR, tail)
    } else {
        format!("{root}{tail}")
    }
}

fn feature(n: &str, o: &Options<'_>) -> bool {
    match n {
        "is_demo_user" => o.demo,
        "has_custom_resolution" => o.resolution_width.is_some() && o.resolution_height.is_some(),
        "has_quick_plays_support" => o.quick_play_support,
        "is_quick_play_singleplayer" => !o.quick_play_singleplayer.is_empty(),
        "is_quick_play_multiplayer" => !o.quick_play_multiplayer.is_empty(),
        "is_quick_play_realms" => !o.quick_play_realms.is_empty(),
        _ => false,
    }
}
fn matches(rule: &Value, i: &Input<'_>) -> Result<bool, Error> {
    let os = &rule["os"];
    if let Some(n) = os["name"].as_str()
        && n != i.install.target.os
    {
        return Ok(false);
    }
    if let Some(p) = os["arch"].as_str()
        && !rules::regex_matches(p, &i.runtime.os_arch)?
    {
        return Ok(false);
    }
    if let Some(p) = os["version"].as_str() {
        let Some(v) = &i.runtime.os_version else {
            return Ok(false);
        };
        if !rules::regex_matches(p, v)? {
            return Ok(false);
        }
    }
    if !os["versionRange"].is_null() {
        let Some(v) = &i.runtime.os_version else {
            return Ok(false);
        };
        if let Some(m) = os["versionRange"]["min"].as_str()
            && rules::compare_versions(v, m)?.is_lt()
        {
            return Ok(false);
        }
        if let Some(m) = os["versionRange"]["max"].as_str()
            && rules::compare_versions(v, m)?.is_gt()
        {
            return Ok(false);
        }
    }
    if let Some(f) = rule["features"].as_object() {
        for (n, v) in f {
            if feature(n, &i.options) != v.as_bool().unwrap_or(false) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn allowed(r: &Value, i: &Input<'_>) -> Result<bool, Error> {
    let Some(r) = r.as_array() else {
        return Ok(true);
    };
    if r.is_empty() {
        return Ok(true);
    }
    let mut a = false;
    for rule in r {
        if matches(rule, i)? {
            a = rule["action"] == "allow"
        }
    }
    Ok(a)
}
fn substitute(t: &str, m: &HashMap<&str, &str>) -> Result<Zeroizing<String>, Error> {
    // Resolve and size before creating secret-bearing storage: no secret reallocations.
    let mut pieces = Vec::new();
    let mut rest = t;
    while let Some(pos) = rest.find("${") {
        pieces.push(&rest[..pos]);
        rest = &rest[pos + 2..];
        let end = rest.find('}').ok_or("InvalidPlaceholder")?;
        let name = &rest[..end];
        if name.is_empty() {
            return Err("InvalidPlaceholder");
        };
        pieces.push(*m.get(name).ok_or("UnknownPlaceholder")?);
        rest = &rest[end + 1..];
    }
    pieces.push(rest);
    let mut out = Zeroizing::new(String::with_capacity(pieces.iter().map(|s| s.len()).sum()));
    for p in pieces {
        out.push_str(p)
    }
    Ok(out)
}
fn append(
    list: &Value,
    out: &mut Vec<Zeroizing<String>>,
    sensitive: &mut Vec<usize>,
    m: &HashMap<&str, &str>,
    i: &Input<'_>,
) -> Result<(), Error> {
    let Some(list) = list.as_array() else {
        return Ok(());
    };
    for a in list {
        let values: Vec<&str> = if let Some(s) = a.as_str() {
            vec![s]
        } else {
            if !allowed(&a["rules"], i)? {
                continue;
            }
            if let Some(s) = a["value"].as_str() {
                vec![s]
            } else {
                a["value"]
                    .as_array()
                    .ok_or("InvalidArguments")?
                    .iter()
                    .map(text)
                    .collect()
            }
        };
        for t in values {
            let v = substitute(t, m)?;
            if t.contains("${auth_access_token}") || t.contains("${auth_session}") {
                sensitive.push(out.len())
            }
            out.push(v)
        }
    }
    Ok(())
}
fn extra(
    args: &[&str],
    out: &mut Vec<Zeroizing<String>>,
    sensitive: &mut Vec<usize>,
    auth: &Auth<'_>,
) {
    for a in args {
        if a.contains(auth.access_token)
            || auth.session.is_some_and(|s| !s.is_empty() && a.contains(s))
        {
            sensitive.push(out.len())
        }
        out.push(Zeroizing::new((*a).into()))
    }
}
pub fn build(i: Input<'_>) -> Result<Plan, Error> {
    let a = &i.auth;
    let o = &i.options;
    let md = i.metadata;
    if a.player_name.is_empty() || a.uuid.is_empty() || a.access_token.is_empty() {
        return Err("InvalidAuthSession");
    }
    let required = md["javaVersion"]["majorVersion"].as_u64().unwrap_or(8);
    if required == 0 {
        return Err("InvalidJavaRequirement");
    }
    if !i.force_java {
        if u64::from(i.runtime.major_version) != required {
            return Err("JavaVersionIncompatible");
        };
        if i.runtime.architecture.as_deref() != Some(&i.install.target.arch) {
            return Err("JavaArchitectureIncompatible");
        }
    }
    let assets = join(i.root_directory, "assets");
    let libraries = join(i.root_directory, "libraries");
    let index = md["assetIndex"]["id"]
        .as_str()
        .or(md["assets"].as_str())
        .unwrap_or("");
    let game_assets = if !index.is_empty()
        && i.install
            .asset_projections
            .iter()
            .any(|p| p.kind == "virtual_assets")
    {
        join(&join(&assets, "virtual"), index)
    } else {
        assets.clone()
    };
    let sep = if i.install.target.os == "windows" {
        ";"
    } else {
        ":"
    };
    let cp = i
        .install
        .classpath
        .iter()
        .map(|p| join(i.root_directory, p))
        .collect::<Vec<_>>()
        .join(sep);
    let width = o
        .resolution_width
        .map(|n| n.to_string())
        .unwrap_or_default();
    let height = o
        .resolution_height
        .map(|n| n.to_string())
        .unwrap_or_default();
    let m = HashMap::from([
        ("auth_player_name", a.player_name),
        ("auth_uuid", a.uuid),
        ("auth_access_token", a.access_token),
        ("auth_session", a.session.unwrap_or(a.access_token)),
        ("user_type", a.user_type),
        ("auth_xuid", a.xuid),
        ("clientid", a.client_id),
        ("user_properties", a.user_properties),
        ("version_name", text(&md["id"])),
        ("version_type", text(&md["type"])),
        ("game_directory", i.game_directory),
        ("assets_root", assets.as_str()),
        ("assets_index_name", index),
        ("game_assets", game_assets.as_str()),
        ("natives_directory", i.natives_directory),
        ("library_directory", libraries.as_str()),
        ("classpath", cp.as_str()),
        ("classpath_separator", sep),
        ("launcher_name", o.launcher_name),
        ("launcher_version", o.launcher_version),
        ("resolution_width", width.as_str()),
        ("resolution_height", height.as_str()),
        ("quickPlayPath", o.quick_play_path),
        ("quickPlaySingleplayer", o.quick_play_singleplayer),
        ("quickPlayMultiplayer", o.quick_play_multiplayer),
        ("quickPlayRealms", o.quick_play_realms),
    ]);
    let (mut jvm, mut game, mut sj, mut sg) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let args = &md["arguments"];
    if !args.is_null() {
        if o.include_default_user_jvm {
            append(&args["default-user-jvm"], &mut jvm, &mut sj, &m, &i)?
        }
        append(&args["jvm"], &mut jvm, &mut sj, &m, &i)?;
        append(&args["game"], &mut game, &mut sg, &m, &i)?
    } else {
        jvm.extend([
            Zeroizing::new(format!("-Djava.library.path={}", i.natives_directory)),
            Zeroizing::new("-cp".into()),
            Zeroizing::new(cp.clone()),
        ]);
        let legacy = md["minecraftArguments"]
            .as_str()
            .ok_or("MissingGameArguments")?;
        for t in rules::tokenize(legacy)? {
            let v = substitute(&t, &m)?;
            if t.contains("${auth_access_token}") || t.contains("${auth_session}") {
                sg.push(game.len())
            }
            game.push(v)
        }
    }
    extra(o.extra_jvm_args, &mut jvm, &mut sj, a);
    if !md["logging"].is_null() {
        let log = &md["logging"]["client"];
        let path = join(&join(&assets, "log_configs"), text(&log["file"]["id"]));
        jvm.push(Zeroizing::new(
            text(&log["argument"]).replace("${path}", &path),
        ))
    }
    extra(o.extra_game_args, &mut game, &mut sg, a);
    let jvm_len = jvm.len();
    let sensitive_argv_indices = sj
        .into_iter()
        .map(|n| n + 1)
        .chain(sg.into_iter().map(|n| n + jvm_len + 2))
        .collect();
    let mut argv = Vec::with_capacity(jvm_len + game.len() + 2);
    argv.push(Zeroizing::new(i.runtime.executable.clone()));
    argv.extend(jvm);
    argv.push(Zeroizing::new(text(&md["mainClass"]).into()));
    argv.extend(game);
    Ok(Plan {
        argv,
        jvm_len,
        working_directory: i.game_directory.into(),
        sensitive_argv_indices,
    })
}

pub mod process;

pub mod events;

pub mod rpc;

pub mod executor;

pub mod cli;
mod offline_uuid;

pub mod instance_install;

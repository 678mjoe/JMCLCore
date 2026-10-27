//! Pure installation planner. No filesystem materialization or network is performed.
use crate::metadata::{Error, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub os: String,
    pub arch: String,
    pub os_version: Option<String>,
}
impl Target {
    pub fn host() -> Result<Self, Error> {
        let os = match std::env::consts::OS {
            "linux" => "linux",
            "macos" => "osx",
            "windows" => "windows",
            _ => return Err(Error::Plan("UnsupportedOperatingSystem")),
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "x86_64" => "x86_64",
            "arm" => "arm",
            "aarch64" => "arm64",
            _ => return Err(Error::Plan("UnsupportedArchitecture")),
        };
        Ok(Self {
            os: os.into(),
            arch: arch.into(),
            os_version: None,
        })
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub client: bool,
    pub asset_index: bool,
    pub library: bool,
    pub native: bool,
    pub logging_config: bool,
    pub asset_object: bool,
    #[serde(rename = "mod")]
    pub mod_file: bool,
    pub resourcepack: bool,
    pub shaderpack: bool,
    pub datapack: bool,
    pub server: bool,
}
impl Usage {
    fn merge(&mut self, x: &Self) {
        self.client |= x.client;
        self.asset_index |= x.asset_index;
        self.library |= x.library;
        self.native |= x.native;
        self.logging_config |= x.logging_config;
        self.asset_object |= x.asset_object;
        self.mod_file |= x.mod_file;
        self.resourcepack |= x.resourcepack;
        self.shaderpack |= x.shaderpack;
        self.datapack |= x.datapack;
        self.server |= x.server;
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub url: String,
    pub fallback_url: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub usage: Usage,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeExtraction {
    pub archive_path: String,
    pub exclude: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetProjection {
    pub source_path: String,
    pub destination_path: String,
    pub kind: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub version_id: String,
    pub source: Source,
    pub target: Target,
    pub files: Vec<File>,
    pub classpath: Vec<String>,
    pub native_extractions: Vec<NativeExtraction>,
    pub asset_projections: Vec<AssetProjection>,
    pub total_download_size: u64,
    pub java_major_version: u16,
    pub loader: String,
}
struct Builder {
    plan: Plan,
    paths: HashMap<String, usize>,
    classpath: HashSet<String>,
    projections: HashMap<String, usize>,
}
fn s(v: &Value) -> &str {
    v.as_str().expect("normalized string")
}
fn n(v: &Value) -> u64 {
    v.as_u64().expect("normalized integer")
}
fn err(name: &'static str) -> Error {
    Error::Plan(name)
}
impl Builder {
    fn new(version: &Value, source: Source, target: Target) -> Self {
        Self {
            plan: Plan {
                version_id: s(&version["id"]).into(),
                source,
                target,
                files: vec![],
                classpath: vec![],
                native_extractions: vec![],
                asset_projections: vec![],
                total_download_size: 0,
                java_major_version: version["javaVersion"]["majorVersion"].as_u64().unwrap_or(0)
                    as u16,
                loader: "none".into(),
            },
            paths: HashMap::new(),
            classpath: HashSet::new(),
            projections: HashMap::new(),
        }
    }
    fn add(&mut self, path: String, item: &Value, usage: Usage) -> Result<String, Error> {
        self.add_file(
            path,
            s(&item["url"]),
            s(&item["sha1"]),
            n(&item["size"]),
            usage,
        )
    }
    fn add_file(
        &mut self,
        path: String,
        url: &str,
        sha1: &str,
        size: u64,
        usage: Usage,
    ) -> Result<String, Error> {
        validate_relative_path(&path)?;
        if !url.starts_with("https://") {
            return Err(err("InsecureDownloadUrl"));
        }
        if sha1.len() != 40 || !sha1.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(err("InvalidSha1"));
        }
        if let Some(&i) = self.paths.get(&path) {
            let old = &mut self.plan.files[i];
            if !old.sha1.eq_ignore_ascii_case(sha1) || old.size != size {
                return Err(err("ConflictingDestination"));
            }
            old.usage.merge(&usage);
            return Ok(path);
        }
        if self.plan.files.len() >= 10_000 {
            return Err(err("TooManyFiles"));
        }
        let mirror = self.plan.source.rewrite(url);
        self.paths.insert(path.clone(), self.plan.files.len());
        self.plan.files.push(File {
            path: path.clone(),
            url: mirror.clone().unwrap_or_else(|| url.into()),
            fallback_url: mirror.map(|_| url.into()),
            sha1: sha1.into(),
            size,
            usage,
        });
        self.plan.total_download_size = self
            .plan
            .total_download_size
            .checked_add(size)
            .ok_or(err("DownloadSizeOverflow"))?;
        Ok(path)
    }
    fn classpath(&mut self, path: String) {
        if self.classpath.insert(path.clone()) {
            self.plan.classpath.push(path)
        }
    }
    fn projection(&mut self, source: &str, dest: String, kind: &str) -> Result<(), Error> {
        validate_relative_path(&dest)?;
        if let Some(i) = self.projections.get(&dest) {
            if self.plan.asset_projections[*i].source_path != source {
                return Err(err("ConflictingProjection"));
            }
            return Ok(());
        }
        if self.plan.asset_projections.len() >= 20_000 {
            return Err(err("TooManyProjections"));
        }
        self.projections
            .insert(dest.clone(), self.plan.asset_projections.len());
        self.plan.asset_projections.push(AssetProjection {
            source_path: source.into(),
            destination_path: dest,
            kind: kind.into(),
        });
        Ok(())
    }
}
pub fn validate_component(v: &str) -> Result<(), Error> {
    validate_component_bytes(v.as_bytes())
}
// General plan component policy only, not a loader ID or archive policy.
// Accepting bytes here does not construct or authorize a native path.
fn validate_component_bytes(v: &[u8]) -> Result<(), Error> {
    if v.is_empty()
        || v == b"."
        || v == b".."
        || matches!(v.last(), Some(b'.' | b' '))
        || v.iter()
            .any(|&c| c < 0x20 || c == 0x7f || matches!(c, b'/' | b'\\' | b':'))
    {
        return Err(err("UnsafePath"));
    }
    let stem = v.split(|&c| c == b'.').next().unwrap().to_ascii_uppercase();
    if matches!(stem.as_slice(), b"CON" | b"PRN" | b"AUX" | b"NUL")
        || (stem.len() == 4
            && (stem.starts_with(b"COM") || stem.starts_with(b"LPT"))
            && matches!(stem[3], b'1'..=b'9'))
    {
        return Err(err("UnsafePath"));
    }
    Ok(())
}
pub fn validate_relative_path(v: &str) -> Result<(), Error> {
    if v.is_empty() || v.starts_with(['/', '\\']) || v.contains('\\') {
        return Err(err("UnsafePath"));
    }
    for c in v.split('/') {
        validate_component(c)?
    }
    Ok(())
}
fn archive_prefix(v: &str) -> Result<(), Error> {
    if v.is_empty() || v.starts_with(['/', '\\']) || v.contains("..") || v.contains('\\') {
        Err(err("UnsafeArchivePath"))
    } else {
        Ok(())
    }
}
fn arch_matches(pattern: &str, arch: &str) -> bool {
    match arch {
        "x86" => matches!(pattern, "x86" | "i386"),
        "x86_64" => matches!(pattern, "x86_64" | "amd64"),
        "arm" => pattern == "arm",
        "arm64" => matches!(pattern, "arm64" | "aarch64"),
        _ => false,
    }
}
pub fn rules_allow(rules: &Value, target: &Target) -> Result<bool, Error> {
    if rules.is_null() {
        return Ok(true);
    }
    let rules = rules.as_array().ok_or(Error::InvalidJson)?;
    if rules.is_empty() {
        return Ok(true);
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, target)? {
            allowed = rule["action"] == "allow"
        }
    }
    Ok(allowed)
}
fn rule_matches(rule: &Value, target: &Target) -> Result<bool, Error> {
    let os = &rule["os"];
    if !os.is_null() {
        if let Some(name) = os["name"].as_str()
            && name != target.os
        {
            return Ok(false);
        }
        if let Some(arch) = os["arch"].as_str()
            && !arch_matches(arch, &target.arch)
        {
            return Ok(false);
        }
        if !os["version"].is_null() || !os["versionRange"].is_null() {
            if target.os_version.is_none() {
                return Ok(false);
            }
            return Err(err("UnsupportedOsVersionRule"));
        }
    }
    if let Some(features) = rule["features"].as_object()
        && features.values().any(|v| v.as_bool() != Some(false))
    {
        return Ok(false);
    }
    Ok(true)
}
pub fn build(
    version: &Value,
    assets: Option<&Value>,
    source: Source,
    target: Target,
) -> Result<Plan, Error> {
    build_with_extras(version, assets, source, target, &[])
}
pub fn build_with_extras(
    version: &Value,
    assets: Option<&Value>,
    source: Source,
    target: Target,
    extras: &[File],
) -> Result<Plan, Error> {
    validate_component(s(&version["id"]))?;
    let mut b = Builder::new(version, source, target.clone());
    let id = s(&version["id"]);
    let client = b.add(
        format!("versions/{id}/{id}.jar"),
        &version["downloads"]["client"],
        Usage {
            client: true,
            ..Default::default()
        },
    )?;
    let descriptor = &version["assetIndex"];
    if !descriptor.is_null() {
        let asset_id = s(&descriptor["id"]);
        validate_component(asset_id)?;
        b.add(
            format!("assets/indexes/{asset_id}.json"),
            descriptor,
            Usage {
                asset_index: true,
                ..Default::default()
            },
        )?;
        if let Some(index) = assets {
            for (logical, object) in index["objects"].as_object().ok_or(Error::InvalidJson)? {
                validate_relative_path(logical)?;
                let hash = s(&object["hash"]);
                if hash.len() != 40 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err(err("InvalidAssetHash"));
                }
                let path = format!("assets/objects/{}/{hash}", &hash[..2]);
                let url = format!(
                    "https://resources.download.minecraft.net/{}/{hash}",
                    &hash[..2]
                );
                let path = b.add_file(
                    path,
                    &url,
                    hash,
                    n(&object["size"]),
                    Usage {
                        asset_object: true,
                        ..Default::default()
                    },
                )?;
                if index["virtual"] == true {
                    b.projection(
                        &path,
                        format!("assets/virtual/{asset_id}/{logical}"),
                        "virtual_assets",
                    )?
                }
                if index["map_to_resources"] == true {
                    b.projection(&path, format!("resources/{logical}"), "resources")?
                }
            }
        }
    } else if assets.is_some() {
        return Err(err("UnexpectedAssetIndex"));
    }
    if !version["logging"].is_null() {
        let file = &version["logging"]["client"]["file"];
        validate_component(s(&file["id"]))?;
        b.add(
            format!("assets/log_configs/{}", s(&file["id"])),
            file,
            Usage {
                logging_config: true,
                ..Default::default()
            },
        )?;
    }
    for lib in version["libraries"].as_array().ok_or(Error::InvalidJson)? {
        if !rules_allow(&lib["rules"], &target)? {
            continue;
        }
        let dl = &lib["downloads"];
        if !lib["natives"].is_null() {
            let Some(template) = lib["natives"][&target.os].as_str() else {
                continue;
            };
            let classifier = template.replace(
                "${arch}",
                if matches!(target.arch.as_str(), "x86" | "arm") {
                    "32"
                } else {
                    "64"
                },
            );
            if dl.is_null() {
                return Err(err("MissingLibraryDownloads"));
            }
            if dl["classifiers"].is_null() {
                return Err(err("MissingNativeClassifiers"));
            }
            let Some(artifact) = dl["classifiers"].get(&classifier) else {
                return Err(err("MissingNativeClassifier"));
            };
            let path = b.add(
                format!("libraries/{}", s(&artifact["path"])),
                artifact,
                Usage {
                    native: true,
                    ..Default::default()
                },
            )?;
            let mut excludes = vec![];
            if let Some(xs) = lib["extract"]["exclude"].as_array() {
                for x in xs {
                    archive_prefix(s(x))?;
                    excludes.push(s(x).into())
                }
            }
            b.plan.native_extractions.push(NativeExtraction {
                archive_path: path,
                exclude: excludes,
            });
            continue;
        }
        if dl.is_null() {
            return Err(err("MissingLibraryDownloads"));
        }
        let artifact = &dl["artifact"];
        if artifact.is_null() {
            return Err(err("MissingLibraryArtifact"));
        }
        let path = b.add(
            format!("libraries/{}", s(&artifact["path"])),
            artifact,
            Usage {
                library: true,
                ..Default::default()
            },
        )?;
        b.classpath(path);
    }
    let cp = if let Some(path) = version["patched_client"].as_str() {
        validate_relative_path(path)?;
        path.into()
    } else {
        client
    };
    b.classpath(cp);
    for x in extras {
        b.add_file(x.path.clone(), &x.url, &x.sha1, x.size, x.usage.clone())?;
    }
    Ok(b.plan)
}
pub fn build_server(
    version: &Value,
    source: Source,
    target: Target,
    extras: &[File],
) -> Result<Plan, Error> {
    validate_component(s(&version["id"]))?;
    let server = &version["downloads"]["server"];
    if server.is_null() {
        return Err(err("ServerDownloadMissing"));
    }
    let java = if version["javaVersion"].is_null() {
        8
    } else {
        let n = n(&version["javaVersion"]["majorVersion"]);
        if n == 0 {
            return Err(err("InvalidJavaRequirement"));
        }
        n as u16
    };
    let mut b = Builder::new(version, source, target);
    b.plan.java_major_version = java;
    b.add(
        "server.jar".into(),
        server,
        Usage {
            server: true,
            ..Default::default()
        },
    )?;
    for x in extras {
        b.add_file(x.path.clone(), &x.url, &x.sha1, x.size, x.usage.clone())?;
    }
    Ok(b.plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata;
    fn minimal() -> Value {
        metadata::metadata(br#"{"id":"1.0","type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":10,"url":"https://piston-data.mojang.com/client"}}}"#).unwrap()
    }
    fn target() -> Target {
        Target {
            os: "linux".into(),
            arch: "x86_64".into(),
            os_version: None,
        }
    }
    #[test]
    fn vanilla_order_and_rewrite() {
        let p = build(&minimal(), None, Source::Bmclapi, target()).unwrap();
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.files[0].url, "https://bmclapi2.bangbang93.com/client");
        assert_eq!(
            p.files[0].fallback_url.as_deref(),
            Some("https://piston-data.mojang.com/client")
        );
        assert_eq!(p.total_download_size, 10);
        assert_eq!(p.classpath, vec!["versions/1.0/1.0.jar"]);
    }
    #[test]
    fn path_safety_and_sha1() {
        for s in ["../x", "a//b", "/x", "CON.txt", "a/b.", "a\\b", "x:y"] {
            assert!(validate_relative_path(s).is_err(), "{s}")
        }
        assert!(validate_relative_path("mod/path.jar").is_ok());
        let mut v = minimal();
        v["downloads"]["client"]["sha1"] = "bad".into();
        assert_eq!(
            build(&v, None, Source::Official, target()),
            Err(err("InvalidSha1"))
        );
    }
    #[test]
    fn rule_order_and_features() {
        let rules =
            serde_json::json!([{"action":"allow"},{"action":"disallow","os":{"name":"linux"}}]);
        assert!(!rules_allow(&rules, &target()).unwrap());
        let rules = serde_json::json!([{"action":"allow","features":{"demo":false}}]);
        assert!(rules_allow(&rules, &target()).unwrap());
    }
    #[test]
    fn dedup_and_conflict() {
        let mut b = Builder::new(&minimal(), Source::Official, target());
        b.add_file(
            "x".into(),
            "https://x",
            &"a".repeat(40),
            3,
            Usage {
                library: true,
                ..Default::default()
            },
        )
        .unwrap();
        b.add_file(
            "x".into(),
            "https://y",
            &"A".repeat(40),
            3,
            Usage {
                client: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(b.plan.total_download_size, 3);
        assert!(b.plan.files[0].usage.library && b.plan.files[0].usage.client);
        assert_eq!(
            b.add_file(
                "x".into(),
                "https://y",
                &"b".repeat(40),
                3,
                Usage::default()
            ),
            Err(err("ConflictingDestination"))
        );
    }
}

#[cfg(test)]
#[path = "plan_component_tests.rs"]
mod component_tests;

#[cfg(test)]
#[path = "plan_byte_tests.rs"]
mod byte_tests;

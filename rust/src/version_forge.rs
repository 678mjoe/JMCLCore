//! Modern and legacy Forge metadata policy; processor execution is separate.
use super::{Policy, Service, Transport, fabric, neoforge as neo, zip};
use crate::{
    metadata::{self, Error, Source},
    plan::{self, Target},
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
fn fail(name: &'static str) -> Error {
    Error::Plan(name)
}
const UPGRADES: [(&str, &str); 5] = [
    (
        "http://files.minecraftforge.net/maven/",
        "https://maven.minecraftforge.net/",
    ),
    (
        "http://maven.minecraftforge.net/",
        "https://maven.minecraftforge.net/",
    ),
    (
        "http://libraries.minecraft.net/",
        "https://libraries.minecraft.net/",
    ),
    (
        "http://repo1.maven.org/maven2/",
        "https://repo1.maven.org/maven2/",
    ),
    (
        "http://maven-central.storage-download.googleapis.com/maven2/",
        "https://maven-central.storage-download.googleapis.com/maven2/",
    ),
];
fn repository(u: &str) -> Result<(), Error> {
    if u.len() == 8
        || u[8..].starts_with('/')
        || u[8..].contains("..")
        || !u
            .bytes()
            .all(|c| (0x21..=0x7e).contains(&c) && !b"\\?#@".contains(&c))
    {
        return Err(fail("InvalidRepositoryUrl"));
    }
    Ok(())
}
pub fn normalized_base(url: Option<&str>) -> Result<String, Error> {
    let Some(url) = url else {
        return Ok("https://libraries.minecraft.net/".into());
    };
    if url.starts_with("https://") {
        repository(url)?;
        return Ok(url.into());
    }
    UPGRADES
        .iter()
        .find(|(a, _)| *a == url)
        .map(|(_, b)| (*b).into())
        .ok_or(fail("InsecureLibraryUrl"))
}
pub fn normalized_artifact(url: &str) -> Result<String, Error> {
    if url.starts_with("https://") {
        repository(url)?;
        return Ok(url.into());
    }
    for (a, b) in UPGRADES {
        if let Some(rest) = url.strip_prefix(a) {
            let url = format!("{b}{rest}");
            repository(&url)?;
            return Ok(url);
        }
    }
    Err(fail("InsecureArtifactUrl"))
}
pub fn universal(name: &str) -> Result<String, Error> {
    neo::artifact_path(name)?;
    let raw = name.rsplit_once('@').map(|x| x.0).unwrap_or(name);
    if raw.split(':').count() == 4 {
        return Ok(name.into());
    }
    if !neo::artifact_path(name)?.ends_with(".jar") {
        return Err(fail("UnsupportedLibraryExtension"));
    }
    Ok(format!("{name}:universal"))
}
fn artifact_url(base: &str, name: &str) -> Result<String, Error> {
    repository(base)?;
    Ok(format!(
        "{}{}{}",
        base,
        if base.ends_with('/') { "" } else { "/" },
        neo::artifact_path(name)?
    ))
}
fn main_class(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 512
        || !value
            .bytes()
            .all(|c| (0x21..0x7f).contains(&c) && !b"/\\".contains(&c))
    {
        return Err(fail("InvalidMainClass"));
    }
    Ok(())
}
fn arguments(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 16384 || !value.bytes().all(|c| (0x20..0x7f).contains(&c))
    {
        return Err(fail("InvalidMinecraftArguments"));
    }
    Ok(())
}
pub fn legacy(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(fail("ProfileTooLarge"));
    }
    let p = metadata::forge_legacy(bytes)?;
    let universal = p["install"]["path"].as_str().unwrap();
    if !neo::artifact_path(universal)?.ends_with(".jar") {
        return Err(fail("UnsupportedLibraryExtension"));
    }
    neo::version_id(p["install"]["minecraft"].as_str().unwrap())?;
    let info = &p["versionInfo"];
    neo::version_id(info["id"].as_str().unwrap())?;
    if let Some(v) = info["inheritsFrom"].as_str() {
        neo::version_id(v)?;
    }
    main_class(info["mainClass"].as_str().unwrap())?;
    arguments(info["minecraftArguments"].as_str().unwrap())?;
    let libs = info["libraries"].as_array().unwrap();
    if libs.is_empty() || libs.len() > 1024 {
        return Err(fail("InvalidLibraryCount"));
    }
    let mut seen = false;
    for l in libs {
        let name = l["name"].as_str().unwrap();
        if !neo::artifact_path(name)?.ends_with(".jar") {
            return Err(fail("UnsupportedLibraryExtension"));
        }
        normalized_base(l["url"].as_str())?;
        if let Some(hashes) = l["checksums"].as_array() {
            if hashes.is_empty() || hashes.len() > 16 {
                return Err(fail("InvalidLibraryChecksums"));
            }
            for hash in hashes {
                fabric::sha1(hash.as_str().unwrap())?;
            }
        }
        seen |= name == universal;
    }
    if !seen {
        return Err(fail("UniversalLibraryMissing"));
    }
    Ok(p)
}
pub fn modern(bytes: &[u8], patched: &str) -> Result<Value, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(fail("VersionProfileTooLarge"));
    }
    let p = metadata::neoforge_version(bytes)?;
    neo::version_id(p["id"].as_str().unwrap())?;
    neo::version_id(p["inheritsFrom"].as_str().unwrap())?;
    main_class(p["mainClass"].as_str().unwrap())?;
    let libs = p["libraries"].as_array().unwrap();
    if libs.len() > 1024 {
        return Err(fail("TooManyLibraries"));
    }
    let patched = neo::artifact_path(patched)?;
    for l in libs {
        let path = neo::artifact_path(l["name"].as_str().unwrap())?;
        if !path.ends_with(".jar") {
            return Err(fail("UnsupportedLibraryExtension"));
        }
        if l["downloads"].is_null() {
            return Err(fail("MissingLibraryDownloads"));
        }
        let a = &l["downloads"]["artifact"];
        if a.is_null() {
            return Err(fail("MissingLibraryArtifact"));
        }
        fabric::sha1(a["sha1"].as_str().unwrap())?;
        fabric::size(a["size"].as_u64().unwrap())?;
        let url = a["url"].as_str().unwrap();
        if url.is_empty() {
            if path != patched {
                return Err(fail("MissingLibraryArtifactUrl"));
            }
            continue;
        }
        if !url.starts_with("https://") {
            return Err(fail("InsecureArtifactUrl"));
        }
        repository(url).map_err(|_| fail("InvalidArtifactUrl"))?;
    }
    Ok(p)
}
fn library_value(name: &Value, path: String, url: String, hash: String, size: u64) -> Value {
    json!({"name":name,"downloads":{"artifact":{"path":path,"sha1":hash,"size":size,"url":url},"classifiers":null},"rules":null,"natives":null,"extract":null,"url":null})
}
fn legacy_merge(parent: &Value, info: &Value, mut libs: Vec<Value>) -> Value {
    let mut out = parent.clone();
    libs.extend(parent["libraries"].as_array().unwrap().iter().cloned());
    out["libraries"] = libs.into();
    for key in ["id", "mainClass"] {
        out[key] = info[key].clone();
    }
    for key in ["type", "releaseTime", "time", "minecraftArguments"] {
        if !info[key].is_null() {
            out[key] = info[key].clone();
        }
    }
    out["arguments"] = Value::Null;
    out["inheritsFrom"] = Value::Null;
    out["patched_client"] = Value::Null;
    out
}
pub struct Resolved {
    pub metadata: Value,
    pub install: Option<Value>,
    pub hash: String,
    pub size: u64,
    pub kind: &'static str,
}
impl<T: Transport> Service<T> {
    pub fn resolve_forge(&self, id: &str, loader: &str, source: Source) -> Result<Resolved, Error> {
        neo::version_id(loader).map_err(|_| fail("InvalidForgeVersion"))?;
        let parent = self.get(id, source)?;
        neo::version_id(id)?;
        let base = "https://maven.minecraftforge.net/net/minecraftforge/forge";
        let url = format!("{base}/{id}-{loader}/forge-{id}-{loader}-installer.jar");
        let bytes = self
            .cached(&url, 64 * 1024 * 1024, Policy::Immutable)
            .or_else(|e| {
                if e == Error::UnsuccessfulHttpStatus {
                    self.cached(
                        &format!(
                            "{base}/{id}-{loader}-{id}/forge-{id}-{loader}-{id}-installer.jar"
                        ),
                        64 * 1024 * 1024,
                        Policy::Immutable,
                    )
                } else {
                    Err(e)
                }
            })?
            .0;
        let install_bytes = zip::read_entry(&bytes, "install_profile.json", 16 * 1024 * 1024)?
            .ok_or(fail("ForgeInstallProfileMissing"))?;
        let probe = metadata::parse_json(&install_bytes)?;
        let hash = format!("{:x}", Sha1::digest(&bytes));
        let size = bytes.len() as u64;
        if !probe["versionInfo"].is_null() {
            let p = legacy(&install_bytes)?;
            let info = &p["versionInfo"];
            if p["install"]["minecraft"] != id
                || info["inheritsFrom"].as_str().is_some_and(|s| s != id)
            {
                return Err(fail("ForgeParentMismatch"));
            }
            let mut libs = Vec::new();
            for l in info["libraries"].as_array().unwrap() {
                let name = l["name"].as_str().unwrap();
                let is_universal = l["name"] == p["install"]["path"];
                let coord = if is_universal {
                    universal(name)?
                } else {
                    name.into()
                };
                let base = normalized_base(l["url"].as_str())?;
                let url = artifact_url(&base, &coord)?;
                let hash = if let Some(checksums) = l["checksums"].as_array() {
                    checksums[0].as_str().unwrap().into()
                } else {
                    self.forge_sidecar(&url).map_err(|e| {
                        if is_universal {
                            fail("LegacyForgeArtifactUnavailable")
                        } else {
                            e
                        }
                    })?
                };
                let size = self.content_length(&url).map_err(|e| {
                    if is_universal {
                        fail("LegacyForgeArtifactUnavailable")
                    } else {
                        e
                    }
                })?;
                fabric::size(size)?;
                libs.push(library_value(
                    &l["name"],
                    neo::artifact_path(&coord)?,
                    url,
                    hash,
                    size,
                ));
            }
            return Ok(Resolved {
                metadata: legacy_merge(&parent, info, libs),
                install: None,
                hash,
                size,
                kind: "forge_legacy",
            });
        }
        let vp = zip::read_entry(&bytes, "version.json", 4 * 1024 * 1024)?
            .ok_or(fail("ForgeVersionProfileMissing"))?;
        let raw_install = metadata::neoforge_install(&install_bytes)?;
        if raw_install["processors"].as_array().unwrap().is_empty()
            && raw_install["data"].as_object().unwrap().is_empty()
        {
            let p = metadata::neoforge_version(&vp)?;
            let expected = format!("{id}-forge-{loader}");
            if raw_install["minecraft"] != id || p["inheritsFrom"] != id {
                return Err(fail("ForgeParentMismatch"));
            }
            if raw_install["version"] != expected || p["id"] != expected {
                return Err(fail("ForgeProfileIdMismatch"));
            }
            neo::version_id(p["id"].as_str().unwrap())?;
            main_class(p["mainClass"].as_str().unwrap())?;
            if let Some(a) = p["minecraftArguments"].as_str() {
                arguments(a)?;
            }
            let original = p["libraries"].as_array().unwrap();
            if original.is_empty() || original.len() > 1024 {
                return Err(fail("TooManyLibraries"));
            }
            let universal_name = format!("net.minecraftforge:forge:{id}-{loader}");
            let mut libs = Vec::new();
            for l in original {
                let name = l["name"].as_str().unwrap();
                let coord = if name == universal_name {
                    universal(name)?
                } else {
                    name.into()
                };
                let path = neo::artifact_path(&coord)?;
                if l["downloads"].is_null() {
                    return Err(fail("MissingLibraryDownloads"));
                }
                let a = &l["downloads"]["artifact"];
                if a.is_null() {
                    return Err(fail("MissingLibraryArtifact"));
                }
                let hash = a["sha1"].as_str().unwrap();
                fabric::sha1(hash)?;
                let size = a["size"].as_u64().unwrap();
                fabric::size(size)?;
                let url = a["url"].as_str().unwrap();
                let url = if url.is_empty() && name == universal_name {
                    artifact_url("https://maven.minecraftforge.net/", &coord)?
                } else {
                    normalized_artifact(url)?
                };
                libs.push(library_value(&l["name"], path, url, hash.into(), size));
            }
            return Ok(Resolved {
                metadata: legacy_merge(&parent, &p, libs),
                install: None,
                hash,
                size,
                kind: "forge_legacy",
            });
        }
        let install = neo::install(&install_bytes)?;
        let patched_path = neo::patched_path(&install, false)?;
        let coordinate = match neo::classify(install["data"]["PATCHED"]["client"].as_str().unwrap())
        {
            neo::Data::Coordinate(c) => c,
            _ => return Err(fail("MissingPatchedData")),
        };
        let mut p = modern(&vp, coordinate)?;
        if install["minecraft"] != id || p["inheritsFrom"] != id {
            return Err(fail("ForgeParentMismatch"));
        }
        let expected = format!("{id}-forge-{loader}");
        if install["version"] != expected || p["id"] != expected {
            return Err(fail("ForgeProfileIdMismatch"));
        }
        let artifact = neo::artifact_path(coordinate)?;
        p["libraries"]
            .as_array_mut()
            .unwrap()
            .retain(|l| neo::artifact_path(l["name"].as_str().unwrap()).unwrap() != artifact);
        let merged = neo::merge(&parent, &p, &patched_path)?;
        Ok(Resolved {
            metadata: merged,
            install: Some(install),
            hash,
            size,
            kind: "forge",
        })
    }
    fn forge_sidecar(&self, url: &str) -> Result<String, Error> {
        let bytes = self
            .cached(&format!("{url}.sha1"), 4096, Policy::Immutable)?
            .0;
        let token = bytes
            .split(|c| b" \t\r\n".contains(c))
            .next()
            .unwrap_or_default();
        let hash = std::str::from_utf8(token).map_err(|_| fail("InvalidLibrarySha1"))?;
        fabric::sha1(hash)?;
        Ok(hash.into())
    }
    pub fn resolve_forge_preferred(
        &self,
        id: &str,
        loader: &str,
        source: Source,
    ) -> Result<Resolved, Error> {
        self.resolve_forge(id, loader, source).or_else(|e| {
            if source == Source::Bmclapi {
                self.resolve_forge(id, loader, Source::Official)
            } else {
                Err(e)
            }
        })
    }
    pub fn plan_forge(
        &self,
        id: &str,
        loader: &str,
        source: Source,
        target: Target,
    ) -> Result<plan::Plan, Error> {
        self.plan_forge_from(id, loader, source, source, target.clone())
            .or_else(|e| {
                if source == Source::Bmclapi {
                    self.plan_forge_from(id, loader, Source::Official, source, target)
                } else {
                    Err(e)
                }
            })
    }
    fn plan_forge_from(
        &self,
        id: &str,
        loader: &str,
        meta_source: Source,
        source: Source,
        target: Target,
    ) -> Result<plan::Plan, Error> {
        let r = self.resolve_forge(id, loader, meta_source)?;
        let assets = if r.metadata["assetIndex"].is_null() {
            None
        } else {
            Some(self.assets(&r.metadata["assetIndex"], meta_source)?)
        };
        let mut extras = Vec::new();
        if let Some(install) = r.install {
            extras.push(plan::File{path:format!(".jmcl/forge/{id}-{loader}/installer.jar"),url:format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{id}-{loader}/forge-{id}-{loader}-installer.jar"),fallback_url:None,sha1:r.hash,size:r.size,usage:plan::Usage{library:true,..Default::default()}});
            for l in install["libraries"].as_array().unwrap() {
                let a = &l["downloads"]["artifact"];
                let path = format!(
                    "libraries/{}",
                    neo::artifact_path(l["name"].as_str().unwrap())?
                );
                if let Some(existing) = extras.iter().find(|f| f.path == path) {
                    if !existing
                        .sha1
                        .eq_ignore_ascii_case(a["sha1"].as_str().unwrap())
                        || existing.size != a["size"].as_u64().unwrap()
                    {
                        return Err(fail("ConflictingDestination"));
                    }
                    continue;
                }
                extras.push(plan::File {
                    path,
                    url: a["url"].as_str().unwrap().into(),
                    fallback_url: None,
                    sha1: a["sha1"].as_str().unwrap().into(),
                    size: a["size"].as_u64().unwrap(),
                    usage: plan::Usage {
                        library: true,
                        ..Default::default()
                    },
                });
            }
        }
        let mut plan =
            plan::build_with_extras(&r.metadata, assets.as_ref(), source, target, &extras)?;
        plan.loader = r.kind.into();
        Ok(plan)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_upgrade_is_exact_and_artifact_upgrade_is_prefix() {
        assert_eq!(
            normalized_base(Some("http://files.minecraftforge.net/maven/")).unwrap(),
            "https://maven.minecraftforge.net/"
        );
        assert!(normalized_base(Some("http://files.minecraftforge.net/maven/extra/")).is_err());
        assert_eq!(
            normalized_artifact("http://files.minecraftforge.net/maven/a.jar").unwrap(),
            "https://maven.minecraftforge.net/a.jar"
        );
        assert!(normalized_artifact("http://files.minecraftforge.net.evil/maven/a.jar").is_err());
        assert_eq!(
            artifact_url("https://maven.minecraftforge.net/", "a:b:c").unwrap(),
            "https://maven.minecraftforge.net/a/b/c/b-c.jar"
        );
        assert_eq!(
            artifact_url("https://maven.minecraftforge.net", "a:b:c").unwrap(),
            "https://maven.minecraftforge.net/a/b/c/b-c.jar"
        );
        assert_eq!(universal("a:b:c").unwrap(), "a:b:c:universal");
        assert_eq!(universal("a:b:c:already").unwrap(), "a:b:c:already");
    }
}
#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::cell::RefCell;
    struct Mock {
        primary: Error,
        calls: RefCell<Vec<String>>,
    }
    impl Transport for Mock {
        fn get(&self, url: &str, _max: usize) -> Result<Vec<u8>, Error> {
            self.calls.borrow_mut().push(url.into());
            if url.ends_with("version_manifest_v2.json") {
                return Ok(br#"{"latest":{"release":"demo","snapshot":"demo"},"versions":[{"id":"demo","type":"release","url":"https://example.invalid/version","time":"x","releaseTime":"x"}]}"#.to_vec());
            }
            if url == "https://example.invalid/version" {
                return Ok(br#"{"id":"demo","type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/client"}}}"#.to_vec());
            }
            if url.contains("demo-2-demo") {
                Ok(b"not a zip".to_vec())
            } else {
                Err(self.primary.clone())
            }
        }
    }
    #[test]
    fn only_http_status_triggers_alternate_installer_url() {
        for (error, alternate) in [
            (Error::UnsuccessfulHttpStatus, true),
            (Error::Network, false),
            (Error::MetadataFetchTimeout, false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let service = Service::new(
                Mock {
                    primary: error.clone(),
                    calls: RefCell::new(vec![]),
                },
                dir.path(),
            );
            let result = service.resolve_forge("demo", "2", Source::Official);
            assert_eq!(
                result.err(),
                Some(if alternate {
                    fail("ZipNoEndRecord")
                } else {
                    error
                })
            );
            assert_eq!(
                service
                    .transport
                    .calls
                    .borrow()
                    .iter()
                    .any(|s| s.contains("demo-2-demo")),
                alternate
            );
        }
    }
}

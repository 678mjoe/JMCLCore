//! Dedicated-server plans share immutable artifacts but not client assets/natives.
use super::{Policy, Service, Transport, fabric, neoforge};
use crate::{
    metadata::{Error, Source},
    plan::{self, Plan, Target},
    servers::manifest::Manifest,
};
use serde_json::Value;
fn fail(s: &'static str) -> Error {
    Error::Plan(s)
}
fn library_file(name: &str, artifact: &Value) -> Result<plan::File, Error> {
    Ok(plan::File {
        path: format!("libraries/{}", neoforge::artifact_path(name)?),
        url: artifact["url"]
            .as_str()
            .ok_or(fail("InvalidLibraryUrl"))?
            .into(),
        fallback_url: None,
        sha1: artifact["sha1"]
            .as_str()
            .ok_or(fail("InvalidLibrarySha1"))?
            .into(),
        size: artifact["size"]
            .as_u64()
            .ok_or(fail("InvalidLibrarySize"))?,
        usage: plan::Usage {
            library: true,
            ..Default::default()
        },
    })
}
fn append_libraries(extras: &mut Vec<plan::File>, libraries: &[Value]) -> Result<(), Error> {
    for library in libraries {
        let file = library_file(
            library["name"]
                .as_str()
                .ok_or(fail("InvalidMavenCoordinate"))?,
            &library["downloads"]["artifact"],
        )?;
        if let Some(existing) = extras.iter().find(|f| f.path == file.path) {
            if !existing.sha1.eq_ignore_ascii_case(&file.sha1) || existing.size != file.size {
                return Err(fail("ConflictingDestination"));
            }
            continue;
        }
        extras.push(file);
    }
    Ok(())
}
impl<T: Transport> Service<T> {
    pub fn resolve_server_selection(&self, m: &Manifest) -> Result<(), Error> {
        let source = Source::parse(&m.source).ok_or(fail("InvalidSource"))?;
        if let Some(loader) = &m.forge_version {
            self.resolve_forge_preferred(&m.version_id, loader, source)
                .map(|_| ())
        } else if let Some(loader) = &m.neoforge_version {
            self.resolve_neoforge_preferred(&m.version_id, loader, source)
                .map(|_| ())
        } else {
            self.resolve_preferred(&m.version_id, m.fabric_loader.as_deref(), source)
                .map(|_| ())
        }
    }
    pub fn plan_server(&self, m: &Manifest, target: Target) -> Result<Plan, Error> {
        let source = Source::parse(&m.source).ok_or(fail("InvalidSource"))?;
        if let Some(loader) = &m.fabric_loader {
            let metadata = self.get_preferred(&m.version_id, source)?;
            fabric::profile_id(&m.version_id)?;
            fabric::profile_id(loader)?;
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}/{loader}/server/json",
                m.version_id
            );
            let profile = fabric::parse(&self.cached(&url, 4 * 1024 * 1024, Policy::Ttl(600))?.0)?;
            if profile["inheritsFrom"] != m.version_id {
                return Err(fail("FabricParentMismatch"));
            }
            if profile["mainClass"] != "net.fabricmc.loader.impl.launch.knot.KnotServer" {
                return Err(fail("InvalidMainClass"));
            }
            let mut extras = vec![];
            for library in profile["libraries"]
                .as_array()
                .ok_or(fail("InvalidLibraries"))?
            {
                let path = fabric::artifact_path(
                    library["name"]
                        .as_str()
                        .ok_or(fail("InvalidMavenCoordinate"))?,
                )?;
                let url = format!(
                    "{}{}",
                    library["url"].as_str().ok_or(fail("InvalidLibraryUrl"))?,
                    path
                );
                let hash = if let Some(hash) = library["sha1"].as_str() {
                    hash.to_owned()
                } else {
                    let b = self
                        .cached(&format!("{url}.sha1"), 4096, Policy::Immutable)?
                        .0;
                    let token = b
                        .split(|b| b" \t\r\n".contains(b))
                        .next()
                        .unwrap_or_default();
                    std::str::from_utf8(token)
                        .map_err(|_| fail("InvalidLibrarySha1"))?
                        .to_owned()
                };
                fabric::sha1(&hash)?;
                let size = if let Some(size) = library["size"].as_u64() {
                    size
                } else {
                    self.content_length(&url)?
                };
                fabric::size(size)?;
                extras.push(plan::File {
                    path: format!("libraries/{path}"),
                    url,
                    fallback_url: None,
                    sha1: hash,
                    size,
                    usage: plan::Usage {
                        library: true,
                        ..Default::default()
                    },
                });
            }
            let mut plan = plan::build_server(&metadata, source, target, &extras)?;
            plan.loader = "fabric".into();
            return Ok(plan);
        }
        if m.neoforge_version.is_some() || m.forge_version.is_some() {
            return self
                .plan_server_processor(m, source, source, target.clone())
                .or_else(|e| {
                    if source == Source::Bmclapi {
                        self.plan_server_processor(m, Source::Official, source, target)
                    } else {
                        Err(e)
                    }
                });
        }
        plan::build_server(
            &self.get_preferred(&m.version_id, source)?,
            source,
            target,
            &[],
        )
    }
    fn plan_server_processor(
        &self,
        m: &Manifest,
        metadata_source: Source,
        source: Source,
        target: Target,
    ) -> Result<Plan, Error> {
        let parent = self.get(&m.version_id, metadata_source)?;
        let (metadata, install, hash, size, kind, loader) =
            if let Some(loader) = &m.neoforge_version {
                let (meta, install, hash, size) =
                    self.resolve_neoforge(&m.version_id, loader, metadata_source)?;
                (meta, install, hash, size, "neoforge", loader.as_str())
            } else {
                let loader = m
                    .forge_version
                    .as_deref()
                    .ok_or(fail("UnsupportedServerLoader"))?;
                let r = self.resolve_forge(&m.version_id, loader, metadata_source)?;
                if r.kind != "forge" {
                    return Err(fail("UnsupportedServerLoader"));
                }
                (
                    r.metadata,
                    r.install.ok_or(fail("UnsupportedServerLoader"))?,
                    r.hash,
                    r.size,
                    "forge",
                    loader,
                )
            };
        let (path, url) = if kind == "neoforge" {
            (
                format!(".jmcl/neoforge-server/{loader}/installer.jar"),
                format!(
                    "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader}/neoforge-{loader}-installer.jar"
                ),
            )
        } else {
            (
                format!(".jmcl/forge-server/{}-{loader}/installer.jar", m.version_id),
                format!(
                    "https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{loader}/forge-{}-{loader}-installer.jar",
                    m.version_id, m.version_id
                ),
            )
        };
        let mut extras = vec![plan::File {
            path,
            url,
            fallback_url: None,
            sha1: hash,
            size,
            usage: plan::Usage {
                library: true,
                ..Default::default()
            },
        }];
        append_libraries(
            &mut extras,
            install["libraries"]
                .as_array()
                .ok_or(fail("InvalidLibraries"))?,
        )?;
        let merged = metadata["libraries"]
            .as_array()
            .ok_or(fail("InvalidLibraries"))?;
        let parent = parent["libraries"]
            .as_array()
            .ok_or(fail("InvalidLibraries"))?;
        let n = if parent.len() > merged.len() {
            merged.len()
        } else {
            merged.len() - parent.len()
        };
        append_libraries(&mut extras, &merged[..n])?;
        let mut plan = plan::build_server(&metadata, source, target, &extras)?;
        plan.loader = kind.into();
        Ok(plan)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    struct Fixture(HashMap<String, Vec<u8>>);
    impl Transport for Fixture {
        fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, Error> {
            self.0.get(url).cloned().ok_or(Error::Network)
        }
    }
    #[test]
    fn fabric_server_profile_is_separate_and_excludes_client_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let version = json!({"id":"1.21.4","type":"release","mainClass":"Client","downloads":{"server":{"url":"https://example.invalid/server.jar","sha1":"a".repeat(40),"size":7},"client":{"url":"https://example.invalid/client.jar","sha1":"b".repeat(40),"size":9}},"libraries":[],"javaVersion":{"majorVersion":21,"component":"java-runtime-delta"}});
        let bytes = serde_json::to_vec(&version).unwrap();
        use sha1::{Digest, Sha1};
        let hash = format!("{:x}", Sha1::digest(&bytes));
        let manifest = json!({"latest":{"release":"1.21.4","snapshot":"1.21.4"},"versions":[{"id":"1.21.4","type":"release","url":"https://example.invalid/version.json","sha1":hash,"time":"x","releaseTime":"x","complianceLevel":1}]});
        let profile = json!({"id":"fabric-loader-0.16.10-1.21.4","inheritsFrom":"1.21.4","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotServer","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/","sha1":"c".repeat(40),"size":5}]});
        let data = HashMap::from([
            (
                "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            ),
            ("https://example.invalid/version.json".into(), bytes),
            (
                "https://meta.fabricmc.net/v2/versions/loader/1.21.4/0.16.10/server/json".into(),
                serde_json::to_vec(&profile).unwrap(),
            ),
        ]);
        let service = Service::new(Fixture(data), temp.path());
        let m: Manifest = serde_json::from_value(
            json!({"id":"demo","name":"Demo","version_id":"1.21.4","fabric_loader":"0.16.10"}),
        )
        .unwrap();
        let plan = service.plan_server(&m, Target::host().unwrap()).unwrap();
        assert_eq!(plan.loader, "fabric");
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].path, "server.jar");
        assert_eq!(plan.java_major_version, 21);
        assert_eq!(plan.total_download_size, 12);
        assert_eq!(crate::servers::planning::fabric_args(&plan.files).unwrap(),b"-cp libraries/net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar:server.jar\nnet.fabricmc.loader.impl.launch.knot.KnotServer\n");
    }
}

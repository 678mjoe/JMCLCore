//! Pure Mojang runtime planning; no filesystem writes, downloads or JVM execution.
use super::receipt;
use serde_json::Value;
pub const CATALOG_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
pub const KNOWN_COMPONENTS: &[(&str, u16)] = &[
    ("java-runtime-epsilon", 21),
    ("java-runtime-delta", 21),
    ("java-runtime-gamma", 17),
    ("java-runtime-beta", 17),
    ("java-runtime-alpha", 16),
    ("jre-legacy", 8),
];
pub fn known_major(component: &str) -> Option<u16> {
    if component == "java-runtime-gamma-snapshot" {
        Some(17)
    } else {
        KNOWN_COMPONENTS
            .iter()
            .find(|(c, _)| *c == component)
            .map(|(_, m)| *m)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct CatalogPlan {
    pub component: String,
    pub manifest_sha1: String,
    pub manifest_size: u64,
    pub manifest_url: String,
}
pub fn catalog_component(catalog: &Value, platform: &str, component: &str) -> Option<CatalogPlan> {
    let manifest = catalog
        .as_object()?
        .get(platform)?
        .as_object()?
        .get(component)?
        .as_array()?
        .first()?
        .as_object()?
        .get("manifest")?
        .as_object()?;
    let sha1 = manifest.get("sha1")?.as_str()?;
    let size = manifest.get("size")?.as_i64()?;
    let url = manifest.get("url")?.as_str()?;
    if sha1.len() != 40 || size <= 0 || !url.starts_with("https://") {
        return None;
    }
    Some(CatalogPlan {
        component: component.into(),
        manifest_sha1: sha1.into(),
        manifest_size: size as u64,
        manifest_url: url.into(),
    })
}
#[derive(Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub executable: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub struct LinkEntry {
    pub path: String,
    pub target: String,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ManifestEntries {
    pub files: Vec<FileEntry>,
    pub links: Vec<LinkEntry>,
    pub directories: Vec<String>,
    pub bytes_total: u64,
}
pub fn validate_link_target(link_path: &str, target: &str) -> Result<(), &'static str> {
    if target.is_empty()
        || target.len() > 1024
        || target.starts_with('/')
        || target.contains(['\\', ':', '\0'])
    {
        return Err("InvalidLinkTarget");
    }
    let mut depth = link_path
        .rsplit_once('/')
        .map_or(0, |(p, _)| p.split('/').filter(|s| !s.is_empty()).count());
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => depth = depth.checked_sub(1).ok_or("InvalidLinkTarget")?,
            _ => depth += 1,
        }
    }
    Ok(())
}
pub fn manifest_files(manifest: &Value) -> Result<ManifestEntries, &'static str> {
    const E: &str = "InvalidRuntimeManifest";
    let files = manifest
        .as_object()
        .and_then(|o| o.get("files"))
        .and_then(Value::as_object)
        .ok_or(E)?;
    let mut result = ManifestEntries::default();
    for (path, value) in files {
        if !receipt::valid_relative(path) {
            return Err(E);
        }
        let entry = value.as_object().ok_or(E)?;
        let kind = entry.get("type").and_then(Value::as_str).ok_or(E)?;
        match kind {
            "directory" => result.directories.push(path.clone()),
            "link" => {
                let target = entry.get("target").and_then(Value::as_str).ok_or(E)?;
                validate_link_target(path, target).map_err(|_| E)?;
                result.links.push(LinkEntry {
                    path: path.clone(),
                    target: target.into(),
                });
            }
            "file" => {
                let raw = entry
                    .get("downloads")
                    .and_then(Value::as_object)
                    .and_then(|o| o.get("raw"))
                    .and_then(Value::as_object)
                    .ok_or(E)?;
                let sha1 = raw.get("sha1").and_then(Value::as_str).ok_or(E)?;
                let size = raw.get("size").and_then(Value::as_i64).ok_or(E)?;
                let url = raw.get("url").and_then(Value::as_str).ok_or(E)?;
                if sha1.len() != 40 || size < 0 || !url.starts_with("https://") {
                    return Err(E);
                }
                let executable = match entry.get("executable") {
                    None => false,
                    Some(v) => v.as_bool().ok_or(E)?,
                };
                result.bytes_total = result.bytes_total.checked_add(size as u64).ok_or(E)?;
                result.files.push(FileEntry {
                    path: path.clone(),
                    url: url.into(),
                    sha1: sha1.into(),
                    size: size as u64,
                    executable,
                });
            }
            _ => return Err(E),
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn component_order_and_first_entry() {
        assert_eq!(known_major("java-runtime-gamma-snapshot"), Some(17));
        let good = json!({"manifest":{"sha1":"x".repeat(40),"size":1,"url":"https://fixture"}});
        assert!(
            catalog_component(
                &json!({"linux":{"jre-legacy":[good.clone()]}}),
                "linux",
                "jre-legacy"
            )
            .is_some()
        );
        assert!(
            catalog_component(
                &json!({"linux":{"jre-legacy":[{},good]}}),
                "linux",
                "jre-legacy"
            )
            .is_none()
        );
    }
    #[test]
    fn link_confinement() {
        for (path, target) in [
            ("bin/java", "../lib/java"),
            ("lib/x/link", "../../bin/java"),
            ("x", "./a//b"),
        ] {
            assert_eq!(validate_link_target(path, target), Ok(()));
        }
        for (path, target) in [
            ("bin/java", "../../outside"),
            ("x", "../x"),
            ("x", "/absolute"),
            ("x", "C:escape"),
            ("x", "a\\b"),
        ] {
            assert_eq!(validate_link_target(path, target), Err("InvalidLinkTarget"));
        }
    }
    #[test]
    fn manifest_order_types_and_totals() {
        let raw = json!({"sha1":"z".repeat(40),"size":5,"url":"https://fixture"});
        let m = json!({"files":{"bin":{"type":"directory"},"bin/java":{"type":"file","downloads":{"raw":raw},"executable":true},"lib/java":{"type":"link","target":"../bin/java"}}});
        let p = manifest_files(&m).unwrap();
        assert_eq!(p.bytes_total, 5);
        assert!(p.files[0].executable);
        assert_eq!(p.directories, ["bin"]);
        let mut m = m;
        m["files"]["bin/java"]["downloads"]["raw"]["size"] = json!(5.0);
        assert!(manifest_files(&m).is_err());
        m["files"]["bin/java"]["downloads"]["raw"]["size"] = json!(-1);
        assert!(manifest_files(&m).is_err());
    }
}

/// Azul provider query preserves the baseline JRE/package choices and parameter order.
pub fn zulu_query(major: u16, platform: &str) -> Result<String, &'static str> {
    if major == 0 {
        return Err("InvalidJavaRequirement");
    }
    let (os, arch) = receipt::platform_target(platform).ok_or("InvalidRuntimePlatform")?;
    let os = if os == "osx" { "macos" } else { os };
    let bits = if matches!(arch, "x86" | "arm") {
        32
    } else {
        64
    };
    let family = if matches!(arch, "x86" | "x86_64") {
        "x86"
    } else {
        "arm"
    };
    Ok(format!(
        "https://api.azul.com/metadata/v1/zulu/packages/?java_version={major}&os={os}&arch={family}&hw_bitness={bits}&archive_type=tar.gz&java_package_type=jre&javafx_bundled=false&release_status=ga&latest=true&page_size=1"
    ))
}
#[derive(Debug, serde::Deserialize)]
pub struct ZuluPackage {
    pub package_uuid: String,
    pub name: String,
    pub download_url: String,
    pub sha256_hash: String,
    #[serde(deserialize_with = "receipt::de_u64")]
    pub size: u64,
}
pub fn zulu_first_package(bytes: &[u8]) -> Result<String, &'static str> {
    #[derive(serde::Deserialize)]
    struct Item {
        package_uuid: String,
        name: String,
    }
    let raw: Vec<Box<serde_json::value::RawValue>> =
        serde_json::from_slice(bytes).map_err(|_| "RuntimePackageUnavailable")?;
    let mut first = None;
    for raw in raw {
        let item: Item = super::object_params(&raw).map_err(|_| "RuntimePackageUnavailable")?;
        let _ = item.name;
        if first.is_none() {
            first = Some(item.package_uuid);
        }
    }
    first.ok_or("RuntimePackageUnavailable")
}
pub fn zulu_package(bytes: &[u8], uuid: &str) -> Result<ZuluPackage, &'static str> {
    let raw: Box<serde_json::value::RawValue> =
        serde_json::from_slice(bytes).map_err(|_| "RuntimePackageUnavailable")?;
    let p: ZuluPackage = super::object_params(&raw).map_err(|_| "RuntimePackageUnavailable")?;
    if p.package_uuid != uuid
        || !p.download_url.starts_with("https://")
        || p.sha256_hash.len() != 64
        || p.size == 0
        || p.size > 512 * 1024 * 1024
    {
        return Err("RuntimePackageUnavailable");
    }
    Ok(p)
}
#[cfg(test)]
mod zulu_tests {
    use super::*;
    #[test]
    fn provider_query_and_limits() {
        assert!(
            zulu_query(17, "mac-os-arm64")
                .unwrap()
                .contains("os=macos&arch=arm&hw_bitness=64")
        );
        assert_eq!(
            zulu_query(0, "linux").unwrap_err(),
            "InvalidJavaRequirement"
        );
        assert_eq!(
            zulu_first_package(br#"[{"package_uuid":"id","name":"a"}]"#).unwrap(),
            "id"
        );
        assert!(zulu_first_package(br#"[{"package_uuid":"id","name":"a"},{}]"#).is_err());
        assert!(zulu_first_package(br#"[["id","a"]]"#).is_err());
        let mut p = serde_json::json!({"package_uuid":"id","name":"a","download_url":"https://fixture","sha256_hash":"x".repeat(64),"size":"42"});
        assert_eq!(
            zulu_package(&serde_json::to_vec(&p).unwrap(), "id")
                .unwrap()
                .size,
            42
        );
        p["size"] = serde_json::json!(512 * 1024 * 1024u64 + 1);
        assert!(zulu_package(&serde_json::to_vec(&p).unwrap(), "id").is_err());
    }
}
